import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once, EventEmitter } from "node:events";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

const extensionSource = readFileSync(new URL("../.pi/extensions/leaf-review.ts", import.meta.url), "utf8");
let moduleSequence = 0;

// Only the Pi host and terminal launcher are replaced. Filesystem IPC and PID checks are real.
async function harness(t, platform = process.platform) {
	const scratch = mkdtempSync(join(tmpdir(), "leaf-review-test-"));
	const oldEnv = { ...process.env };
	const oldPlatform = Object.getOwnPropertyDescriptor(process, "platform");
	Object.defineProperty(process, "platform", { value: platform, configurable: true });
	process.env.PATH = scratch;
	process.env.LEAF_REVIEW_BIN = process.execPath;
	delete process.env.LEAF_REVIEW_TERMINAL;
	delete process.env.TERMINAL;
	if (platform === "win32") delete process.env.LEAF_REVIEW_LAUNCHER;
	else process.env.LEAF_REVIEW_LAUNCHER = process.execPath;
	const callbacks = new Map();
	const commands = new Map();
	const tools = new Map();
	const notifications = [];
	const messages = [];
	const launches = [];
	let poll;
	let clock = Date.now();
	globalThis.__leafTestHost = {
		Date: class extends Date { static now() { return clock; } },
		spawn(command, args) {
			const child = new EventEmitter();
			child.unref = () => {};
			launches.push({ command, args, child });
			return child;
		},
		setInterval(callback) { poll = callback; return { unref() {} }; },
		clearInterval() {},
	};
	let source = stripTypeScriptTypes(extensionSource);
	source = source.replace('import { spawn } from "node:child_process";',
		'const { spawn, setInterval, clearInterval, Date } = globalThis.__leafTestHost;');
	source = source.replace('import { Type } from "typebox";',
		'const Type = new Proxy({}, { get: () => (...args) => args });');
	source = source.replace(/import\s*\{\s*isToolCallEventType,?\s*\}\s*from\s*"@earendil-works\/pi-coding-agent";/,
		'const isToolCallEventType = (name, event) => name === event.toolName;');
	const { default: extension } = await import(
		`data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${++moduleSequence}`,
	);
	extension({
		on: (name, callback) => callbacks.set(name, callback),
		registerCommand: (name, command) => commands.set(name, command),
		registerTool: (tool) => tools.set(tool.name, tool),
		sendUserMessage: (...args) => messages.push(args),
	});
	const document = join(scratch, "a&calc&b.md");
	writeFileSync(document, "one\ntwo\n");
	const sessionId = `test-${process.pid}-${moduleSequence}`;
	const context = {
		cwd: scratch,
		sessionManager: { getSessionId: () => sessionId },
		isIdle: () => true,
		ui: {
			notify: (...args) => notifications.push(args),
			setStatus() {}, setWorkingMessage() {}, theme: { fg: (_color, text) => text },
		},
	};
	const previousDirectories = new Set(readdirSync(tmpdir()));
	callbacks.get("session_start")({}, context);
	const channelRoots = new Set(readdirSync(tmpdir())
		.filter((name) => !previousDirectories.has(name) && name.startsWith(`leaf-pi-${sessionId}-`))
		.map((name) => join(tmpdir(), name)));
	function rememberRoots() {
		for (const launch of launches) {
			const index = launch.args.indexOf("--review-channel");
			if (index >= 0) channelRoots.add(dirname(launch.args[index + 1]));
		}
	}
	t.after(() => {
		callbacks.get("session_shutdown")();
		rememberRoots();
		for (const root of channelRoots) rmSync(root, { recursive: true, force: true });
		rmSync(scratch, { recursive: true, force: true });
		for (const name of Object.keys(process.env)) if (!(name in oldEnv)) delete process.env[name];
		Object.assign(process.env, oldEnv);
		Object.defineProperty(process, "platform", oldPlatform);
		delete globalThis.__leafTestHost;
	});
	return {
		scratch, document, context, callbacks, launches, notifications, messages, tools,
		async open() { await commands.get("leaf-review").handler(document, context); rememberRoots(); },
		poll: () => poll(),
		advance: (milliseconds) => { clock += milliseconds; },
		channel: () => {
			const args = launches.at(-1).args;
			return args[args.indexOf("--review-channel") + 1];
		},
	};
}

function request(document, id, body = "Clarify") {
	return {
		protocol_version: 1, type: "review_request", request_id: id, submitted_at_ms: Date.now(),
		document: { path: document, filename: "review.md", revision: "r" },
		comments: [{ id: 1, body, target: { source_line: 1, source_line_text: "one", rendered_line: 1 }, context: [] }],
	};
}

function events(channel) {
	return readdirSync(join(channel, "events")).filter((name) => name.endsWith(".json"))
		.map((name) => JSON.parse(readFileSync(join(channel, "events", name), "utf8")));
}

function client(channel, document, pid) {
	writeFileSync(join(channel, "client.json"), JSON.stringify({
		protocol_version: 1, process_id: pid, document_path: document, connected_at_ms: Date.now(),
	}));
}

test("Windows without a safe terminal never falls back to cmd.exe", async (t) => {
	const h = await harness(t, "win32");
	await h.open();
	assert.equal(h.launches.length, 0);
	assert.ok(h.notifications.some(([text]) => text.includes("no supported terminal")));
});

test("Windows Terminal receives shell metacharacters as literal arguments", async (t) => {
	const h = await harness(t, "win32");
	writeFileSync(join(h.scratch, "wt.EXE"), "");
	await h.open();
	assert.equal(h.launches.length, 1);
	assert.equal(h.launches[0].command, "wt");
	assert.equal(h.launches[0].args.at(-1), h.document);
	assert.ok(!h.launches[0].args.includes("/c"));
});

test("oversized and malformed requests receive request-specific failure events", async (t) => {
	const h = await harness(t);
	await h.open();
	const channel = h.channel();
	for (const [id, contents] of [
		["large", JSON.stringify(request(h.document, "large", "x".repeat(1024 * 1024)))],
		["broken", "{not json"],
		["invalid", JSON.stringify({ ...request(h.document, "invalid"), protocol_version: 99 })],
	]) {
		writeFileSync(join(channel, "requests", `request-${id}.json`), contents);
	}
	h.poll();
	assert.equal(h.messages.length, 0);
	assert.deepEqual(events(channel).map((event) => [event.type, event.request_id]).sort(),
		[["review_failed", "broken"], ["review_failed", "invalid"], ["review_failed", "large"]]);
	assert.equal(readdirSync(join(channel, "requests")).length, 0);
});

test("a live Leaf client is reused, a closed client reopens on the next artifact edit", async (t) => {
	const h = await harness(t);
	await h.open();
	const firstChannel = h.channel();
	const leaf = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"]);
	t.after(() => leaf.kill());
	client(firstChannel, h.document, leaf.pid);
	await h.open();
	assert.equal(h.launches.length, 1);
	const exited = once(leaf, "exit");
	leaf.kill();
	await exited;
	h.callbacks.get("tool_call")({ toolName: "write", toolCallId: "write-1", input: { path: h.document } }, h.context);
	h.callbacks.get("tool_execution_end")({ toolCallId: "write-1", isError: false });
	h.callbacks.get("agent_settled")({}, h.context);
	assert.equal(h.launches.length, 2);
	assert.notEqual(h.channel(), firstChannel);
	assert.ok(!existsSync(join(h.channel(), "client.json")));
});

test("a launch without a client expires without duplicating a starting client", async (t) => {
	const h = await harness(t);
	await h.open();
	await h.open();
	assert.equal(h.launches.length, 1);
	h.launches[0].child.emit("exit", 0, null);
	h.advance(10_001);
	await h.open();
	assert.equal(h.launches.length, 2);
});

test("a rejection is retried when its failure event cannot be written", async (t) => {
	const h = await harness(t);
	await h.open();
	const channel = h.channel();
	const eventDirectory = join(channel, "events");
	rmSync(eventDirectory, { recursive: true });
	writeFileSync(eventDirectory, "not a directory");
	const path = join(channel, "requests", "request-retry.json");
	writeFileSync(path, "{broken");
	h.poll();
	assert.ok(existsSync(path));
	rmSync(eventDirectory);
	mkdirSync(eventDirectory);
	h.poll();
	assert.ok(!existsSync(path));
	assert.equal(events(channel)[0].request_id, "retry");
});

test("an exited launcher reports failure and permits retry without confusing launcher and Leaf lifetimes", async (t) => {
	const h = await harness(t);
	await h.open();
	h.launches[0].child.emit("exit", 1, null);
	await h.open();
	assert.equal(h.launches.length, 2);
	assert.ok(h.notifications.some(([text]) => text.includes("could not launch")));
	client(h.channel(), h.document, process.pid);
	h.launches[1].child.emit("exit", 0, null);
	await h.open();
	assert.equal(h.launches.length, 2);
});

test("valid publication still delivers comments and completion to Leaf", async (t) => {
	const h = await harness(t);
	await h.open();
	const channel = h.channel();
	writeFileSync(join(channel, "requests", "request-happy.json"), JSON.stringify(request(h.document, "happy")));
	h.poll();
	assert.equal(h.messages.length, 1);
	assert.ok(h.messages[0][0].includes("Clarify"));
	await h.tools.get("leaf_review_complete").execute("complete", {
		request_id: "happy", addressed_comment_ids: [1],
	}, undefined, undefined, h.context);
	assert.deepEqual(events(channel).map((event) => event.type).sort(), ["review_completed", "review_started"]);
});
