import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import {
	existsSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	realpathSync,
	renameSync,
	statSync,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import {
	basename,
	delimiter,
	dirname,
	extname,
	join,
	resolve,
} from "node:path";
import { Type } from "typebox";
import {
	isToolCallEventType,
	type ExtensionAPI,
	type ExtensionContext,
} from "@earendil-works/pi-coding-agent";

const PROTOCOL_VERSION = 1;
const POLL_INTERVAL_MS = 150;
const MAX_REQUEST_BYTES = 1024 * 1024;
const CLIENT_START_TIMEOUT_MS = 10_000;
const MARKDOWN_EXTENSIONS = new Set([".md", ".mdx", ".markdown"]);
const COMPLETION_TOOL = "leaf_review_complete";

type FileFingerprint = string | null;

type TrackedPath = {
	path: string;
	before: FileFingerprint;
	alwaysReview: boolean;
};

type ReviewChannel = {
	documentPath: string;
	root: string;
	requestsDir: string;
	eventsDir: string;
	startedAtMs: number;
};

type ReviewTarget = {
	source_line: number;
	source_line_text: string;
	source_revision?: string;
	rendered_line: number;
	selection?: {
		text: string;
		rendered_start_column: number;
		rendered_end_column_exclusive: number;
	};
};

type ReviewComment = {
	id: number;
	body: string;
	target: ReviewTarget;
	context: Array<{ source_line: number; text: string; is_target: boolean }>;
};

type ReviewRequest = {
	protocol_version: number;
	type: "review_request";
	request_id: string;
	submitted_at_ms: number;
	document: { path: string; filename: string; revision: string };
	comments: ReviewComment[];
};

type PendingReview = {
	channel: ReviewChannel;
	commentIds: Set<number>;
};

type LaunchSpec = {
	command: string;
	args: string[];
};

function isMarkdownPath(path: string): boolean {
	return MARKDOWN_EXTENSIONS.has(extname(path).toLowerCase());
}

function normalizeToolPath(path: string, cwd: string): string {
	const withoutAt = path.startsWith("@") ? path.slice(1) : path;
	return resolve(cwd, withoutAt);
}

function canonicalPath(path: string): string {
	try {
		return realpathSync(path);
	} catch {
		return resolve(path);
	}
}

function fileFingerprint(path: string): FileFingerprint {
	try {
		const stats = statSync(path);
		return stats.isFile() ? `${stats.mtimeMs}:${stats.size}` : null;
	} catch {
		return null;
	}
}

function shellMarkdownPaths(command: string, cwd: string): string[] {
	const paths = new Set<string>();
	const pattern = /(?:^|[\s"'=<>])([^\s"'<>|;&]+\.(?:md|mdx|markdown))(?=$|[\s"'<>|;&])/gi;
	for (const match of command.matchAll(pattern)) {
		if (match[1]) paths.add(normalizeToolPath(match[1], cwd));
	}
	return [...paths];
}

function commandExists(command: string): boolean {
	if (command.includes("/") || command.includes("\\")) return existsSync(command);
	const directories = (process.env.PATH ?? "").split(delimiter).filter(Boolean);
	const extensions = process.platform === "win32"
		? (process.env.PATHEXT ?? ".EXE;.CMD;.BAT;.COM").split(";")
		: [""];
	return directories.some((directory) =>
		extensions.some((extension) => existsSync(join(directory, `${command}${extension}`))),
	);
}

function writeJsonAtomically(directory: string, filename: string, value: unknown): void {
	mkdirSync(directory, { recursive: true, mode: 0o700 });
	const temporary = join(
		directory,
		`.${filename}.${process.pid}.${randomBytes(4).toString("hex")}.tmp`,
	);
	writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
	try {
		renameSync(temporary, join(directory, filename));
	} catch (error) {
		try {
			unlinkSync(temporary);
		} catch {
			// Best effort cleanup only.
		}
		throw error;
	}
}

function eventFilename(sequence: number): string {
	return `event-${Date.now().toString().padStart(13, "0")}-${sequence
		.toString()
		.padStart(6, "0")}.json`;
}

function resolveLeafBinary(cwd: string): string | null {
	const configured = process.env.LEAF_REVIEW_BIN?.trim();
	if (configured) return commandExists(configured) ? configured : null;

	const localBinary = join(cwd, "target", "debug", process.platform === "win32" ? "leaf.exe" : "leaf");
	if (existsSync(localBinary)) return localBinary;
	return commandExists("leaf") ? "leaf" : null;
}

function terminalLaunchSpec(
	leafBinary: string,
	leafArgs: string[],
	documentPath: string,
): LaunchSpec | null {
	const title = `leaf review: ${basename(documentPath)}`;
	const customLauncher = process.env.LEAF_REVIEW_LAUNCHER?.trim();
	if (customLauncher) {
		return commandExists(customLauncher)
			? { command: customLauncher, args: [leafBinary, ...leafArgs] }
			: null;
	}

	if (process.platform === "darwin") {
		const shellCommand = [leafBinary, ...leafArgs]
			.map((part) => `'${part.replaceAll("'", `'\\''`)}'`)
			.join(" ");
		const escaped = shellCommand.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
		return {
			command: "osascript",
			args: ["-e", `tell application "Terminal" to do script "${escaped}"`],
		};
	}

	if (process.platform === "win32") {
		if (commandExists("wt")) {
			return {
				command: "wt",
				args: ["new-tab", "--title", title, leafBinary, ...leafArgs],
			};
		}
		return null;
	}

	const builders: Record<string, () => LaunchSpec> = {
		"xdg-terminal-exec": () => ({
			command: "xdg-terminal-exec",
			args: [`--title=${title}`, "--", leafBinary, ...leafArgs],
		}),
		footclient: () => ({
			command: "footclient",
			args: ["--no-wait", `--title=${title}`, leafBinary, ...leafArgs],
		}),
		alacritty: () => ({
			command: "alacritty",
			args: ["--title", title, "-e", leafBinary, ...leafArgs],
		}),
		kitty: () => ({
			command: "kitty",
			args: ["--title", title, leafBinary, ...leafArgs],
		}),
		ghostty: () => ({
			command: "ghostty",
			args: [`--title=${title}`, "-e", leafBinary, ...leafArgs],
		}),
		wezterm: () => ({
			command: "wezterm",
			args: ["start", "--cwd", dirname(documentPath), "--", leafBinary, ...leafArgs],
		}),
		"gnome-terminal": () => ({
			command: "gnome-terminal",
			args: [`--title=${title}`, "--", leafBinary, ...leafArgs],
		}),
		konsole: () => ({
			command: "konsole",
			args: ["--new-tab", "-p", `tabtitle=${title}`, "-e", leafBinary, ...leafArgs],
		}),
		xterm: () => ({
			command: "xterm",
			args: ["-T", title, "-e", leafBinary, ...leafArgs],
		}),
	};

	const preferred = process.env.LEAF_REVIEW_TERMINAL?.trim();
	if (preferred) {
		const name = basename(preferred);
		return builders[name] && commandExists(preferred)
			? { ...builders[name](), command: preferred }
			: null;
	}

	const environmentTerminal = process.env.TERMINAL?.trim();
	const environmentName = environmentTerminal ? basename(environmentTerminal) : "";
	const order = [
		...(environmentName && builders[environmentName] ? [environmentName] : []),
		"xdg-terminal-exec",
		"footclient",
		"alacritty",
		"kitty",
		"ghostty",
		"wezterm",
		"gnome-terminal",
		"konsole",
		"xterm",
	];
	for (const name of new Set(order)) {
		const executable = name === environmentName && environmentTerminal ? environmentTerminal : name;
		if (commandExists(executable)) {
			return { ...builders[name](), command: executable };
		}
	}
	return null;
}

function reviewClientState(channel: ReviewChannel): "connected" | "starting" | "closed" {
	const clientPath = join(channel.root, "client.json");
	if (!existsSync(clientPath)) {
		return Date.now() - channel.startedAtMs < CLIENT_START_TIMEOUT_MS ? "starting" : "closed";
	}
	try {
		const client: unknown = JSON.parse(readFileSync(clientPath, "utf8"));
		if (
			!client || typeof client !== "object" ||
			!("protocol_version" in client) || client.protocol_version !== PROTOCOL_VERSION ||
			!("document_path" in client) || typeof client.document_path !== "string" ||
			canonicalPath(client.document_path) !== channel.documentPath ||
			!("process_id" in client) || typeof client.process_id !== "number" ||
			!Number.isSafeInteger(client.process_id) || client.process_id <= 0
		) {
			return "closed";
		}
		try {
			process.kill(client.process_id, 0);
			return "connected";
		} catch (error) {
			return error instanceof Error && "code" in error && error.code === "EPERM"
				? "connected"
				: "closed";
		}
	} catch {
		return "closed";
	}
}

function validReviewRequest(value: unknown, channel: ReviewChannel): value is ReviewRequest {
	if (!value || typeof value !== "object") return false;
	const request = value as Partial<ReviewRequest>;
	if (
		request.protocol_version !== PROTOCOL_VERSION ||
		request.type !== "review_request" ||
		typeof request.request_id !== "string" ||
		!request.document ||
		typeof request.document.path !== "string" ||
		canonicalPath(request.document.path) !== channel.documentPath ||
		!Array.isArray(request.comments) ||
		request.comments.length === 0
	) {
		return false;
	}
	return request.comments.every(
		(comment) =>
			Number.isSafeInteger(comment?.id) &&
			comment.id > 0 &&
			typeof comment.body === "string" &&
			comment.body.trim().length > 0 &&
			Number.isSafeInteger(comment.target?.source_line) &&
			comment.target.source_line > 0 &&
			typeof comment.target.source_line_text === "string",
	);
}

function reviewPrompt(request: ReviewRequest): string {
	return `[Leaf document review]\n\nThe user reviewed the Markdown document below in Leaf and submitted root-level comments. Address these comments in the existing file. Do not add replies to comments or start a discussion thread; this integration currently supports addressing comments only.\n\nDocument: ${request.document.path}\nReview request: ${request.request_id}\nDocument revision at submission: ${request.document.revision}\nEach comment's target.source_revision identifies its original snapshot. Its source/rendered coordinates, source line text, and context refer to that snapshot, not necessarily the current document. Locate the original target by text and context rather than blindly applying stale line numbers.\n\nStructured review metadata:\n\n\`\`\`json\n${JSON.stringify(request, null, 2)}\n\`\`\`\n\nInstructions:\n1. Inspect the current document before editing because it may have changed after the review snapshot.\n2. Address each submitted comment in place, using its source line, selected text, exact source line text, rendered coordinates, and surrounding context to locate the target. Rendered lines and columns are 1-based; rendered_end_column_exclusive is the exclusive endpoint.\n3. Keep unrelated document content intact and validate the resulting Markdown.\n4. Only after the edits are complete, call ${COMPLETION_TOOL} exactly once with this request id and the ids of comments you actually addressed. This completion call keeps Leaf open, refreshes the document, and marks those comments addressed.\n5. If a comment cannot be addressed, omit its id from the completion call and explain why in your final response.`;
}

export default function leafReviewExtension(pi: ExtensionAPI): void {
	let currentContext: ExtensionContext | undefined;
	let sessionRoot: string | undefined;
	let pollTimer: ReturnType<typeof setInterval> | undefined;
	let polling = false;
	let eventSequence = 0;
	const trackedTools = new Map<string, TrackedPath[]>();
	const changedMarkdown = new Set<string>();
	const channels = new Map<string, ReviewChannel>();
	const pendingReviews = new Map<string, PendingReview>();

	function writeEvent(channel: ReviewChannel, event: Record<string, unknown>): void {
		eventSequence += 1;
		writeJsonAtomically(channel.eventsDir, eventFilename(eventSequence), {
			protocol_version: PROTOCOL_VERSION,
			...event,
		});
	}

	function updatePiStatus(ctx: ExtensionContext): void {
		if (pendingReviews.size === 0) {
			ctx.ui.setStatus("leaf-review", undefined);
			ctx.ui.setWorkingMessage();
			return;
		}
		const commentCount = [...pendingReviews.values()].reduce(
			(total, pending) => total + pending.commentIds.size,
			0,
		);
		ctx.ui.setStatus(
			"leaf-review",
			ctx.ui.theme.fg("warning", `Leaf review: ${commentCount} comment${commentCount === 1 ? "" : "s"}`),
		);
		ctx.ui.setWorkingMessage("Addressing Leaf review comments…");
	}

	function failReview(requestId: string, message: string, ctx?: ExtensionContext): void {
		const pending = pendingReviews.get(requestId);
		if (!pending) return;
		try {
			writeEvent(pending.channel, {
				type: "review_failed",
				request_id: requestId,
				message,
			});
		} catch (error) {
			console.error(`Leaf review could not publish failure: ${String(error)}`);
		}
		pendingReviews.delete(requestId);
		if (ctx) updatePiStatus(ctx);
	}

	function ensureReview(documentPath: string, ctx: ExtensionContext): void {
		const canonical = canonicalPath(documentPath);
		const existing = channels.get(canonical);
		if (existing) {
			if (reviewClientState(existing) !== "closed") return;
			channels.delete(canonical);
		}
		if (!sessionRoot) return;

		const leafBinary = resolveLeafBinary(ctx.cwd);
		if (!leafBinary) {
			ctx.ui.notify(
				"Leaf review: leaf executable not found. Set LEAF_REVIEW_BIN to the new leaf binary.",
				"error",
			);
			return;
		}

		const channelId = createHash("sha256").update(canonical).digest("hex").slice(0, 16);
		const root = join(sessionRoot, `${channelId}-${randomBytes(4).toString("hex")}`);
		const channel: ReviewChannel = {
			documentPath: canonical,
			root,
			requestsDir: join(root, "requests"),
			eventsDir: join(root, "events"),
			startedAtMs: Date.now(),
		};
		mkdirSync(channel.requestsDir, { recursive: true, mode: 0o700 });
		mkdirSync(channel.eventsDir, { recursive: true, mode: 0o700 });
		writeJsonAtomically(root, "session.json", {
			protocol_version: PROTOCOL_VERSION,
			pi_session_id: ctx.sessionManager.getSessionId(),
			document_path: canonical,
			created_at_ms: Date.now(),
		});

		const leafArgs = ["--review-channel", root, canonical];
		const launch = terminalLaunchSpec(leafBinary, leafArgs, canonical);
		if (!launch) {
			ctx.ui.notify(
				"Leaf review: no supported terminal launcher found. Set LEAF_REVIEW_LAUNCHER.",
				"error",
			);
			return;
		}

		channels.set(canonical, channel);
		const child = spawn(launch.command, launch.args, {
			cwd: dirname(canonical),
			detached: true,
			stdio: "ignore",
		});
		function launchFailed(message: string): void {
			if (channels.get(canonical) !== channel) return;
			channels.delete(canonical);
			try {
				writeEvent(channel, { type: "bridge_closed", message: `Could not launch Leaf: ${message}` });
			} catch {
				// The Pi notification below is the useful failure path.
			}
			try {
				ctx.ui.notify(`Leaf review could not launch: ${message}`, "error");
			} catch {
				console.error(`Leaf review could not launch: ${message}`);
			}
		}
		child.once("error", (error) => launchFailed(error.message));
		child.once("exit", (code, signal) => {
			if (code !== 0 && reviewClientState(channel) !== "connected") {
				launchFailed(signal ? `launcher terminated by ${signal}` : `launcher exited with code ${code}`);
			}
		});
		child.unref();
		ctx.ui.notify(`Opened Leaf review for ${basename(canonical)}.`, "info");
	}

	function handleRequest(request: ReviewRequest, channel: ReviewChannel, ctx: ExtensionContext): void {
		if (pendingReviews.has(request.request_id)) return;
		if ([...pendingReviews.values()].some((pending) => pending.channel === channel)) {
			writeEvent(channel, {
				type: "review_failed",
				request_id: request.request_id,
				message: "Pi is already addressing another review for this document",
			});
			return;
		}

		const commentIds = new Set(request.comments.map((comment) => comment.id));
		pendingReviews.set(request.request_id, { channel, commentIds });
		try {
			writeEvent(channel, {
				type: "review_started",
				request_id: request.request_id,
			});
			updatePiStatus(ctx);
			pi.sendUserMessage(
				reviewPrompt(request),
				ctx.isIdle() ? undefined : { deliverAs: "followUp" },
			);
		} catch (error) {
			failReview(
				request.request_id,
				`Could not send the review to Pi: ${error instanceof Error ? error.message : String(error)}`,
				ctx,
			);
		}
	}

	function pollRequests(): void {
		if (polling || !currentContext) return;
		polling = true;
		try {
			const ctx = currentContext;
			for (const channel of channels.values()) {
				let names: string[];
				try {
					names = readdirSync(channel.requestsDir)
						.filter((name) => name.endsWith(".json") && !name.startsWith("."))
						.sort();
				} catch {
					continue;
				}
				for (const name of names) {
					const requestPath = join(channel.requestsDir, name);
					const requestId = /^request-([a-zA-Z0-9_-]+)\.json$/.exec(name)?.[1];
					let consumed = false;
					try {
						if (statSync(requestPath).size > MAX_REQUEST_BYTES) {
							throw new Error("review request exceeds 1 MiB");
						}
						const parsed: unknown = JSON.parse(readFileSync(requestPath, "utf8"));
						if (!validReviewRequest(parsed, channel) || parsed.request_id !== requestId) {
							throw new Error("invalid review request or document path");
						}
						handleRequest(parsed, channel, ctx);
						consumed = true;
					} catch (error) {
						const message = error instanceof Error ? error.message : String(error);
						try {
							if (requestId) {
								writeEvent(channel, {
									type: "review_failed",
									request_id: requestId,
									message: `Review request rejected: ${message}`,
								});
							}
							consumed = true;
						} catch (ackError) {
							ctx.ui.notify(`Leaf review could not acknowledge rejection: ${String(ackError)}`, "error");
						}
						ctx.ui.notify(`Leaf review request rejected: ${message}`, "error");
					} finally {
						if (consumed) {
							try {
								unlinkSync(requestPath);
							} catch {
								// A later poll can retry cleanup if the file still exists.
							}
						}
					}
				}
			}
		} finally {
			polling = false;
		}
	}

	pi.registerTool({
		name: COMPLETION_TOOL,
		label: "Complete Leaf Review",
		description:
			"Tell the open Leaf document review which submitted comments were addressed. Call this exactly once, only after editing the reviewed Markdown file.",
		promptSnippet: "Mark submitted Leaf document comments addressed after their file edits are complete",
		promptGuidelines: [
			"Call leaf_review_complete exactly once after completing edits requested by a [Leaf document review] message; report only comment ids actually addressed.",
		],
		parameters: Type.Object({
			request_id: Type.String({ description: "Review request id from the Leaf message." }),
			addressed_comment_ids: Type.Array(
				Type.Integer({ minimum: 1 }),
				{ description: "Ids of comments actually addressed in the document." },
			),
			summary: Type.Optional(
				Type.String({ description: "Short summary of the document changes." }),
			),
		}),
		async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
			const pending = pendingReviews.get(params.request_id);
			if (!pending) {
				throw new Error(`Unknown or already completed Leaf review: ${params.request_id}`);
			}
			const addressed = [...new Set(params.addressed_comment_ids)];
			const unknown = addressed.filter((id) => !pending.commentIds.has(id));
			if (unknown.length > 0) {
				throw new Error(`Comment ids do not belong to this Leaf review: ${unknown.join(", ")}`);
			}

			writeEvent(pending.channel, {
				type: "review_completed",
				request_id: params.request_id,
				addressed_comment_ids: addressed,
				...(params.summary ? { message: params.summary } : {}),
			});
			pendingReviews.delete(params.request_id);
			updatePiStatus(ctx);
			ctx.ui.notify(
				`Leaf refreshed; ${addressed.length} comment${addressed.length === 1 ? "" : "s"} marked addressed.`,
				"info",
			);
			return {
				content: [
					{
						type: "text" as const,
						text: `Leaf was notified. Marked ${addressed.length} of ${pending.commentIds.size} submitted comments addressed; the review application remains open and watches the document.`,
					},
				],
				details: {
					requestId: params.request_id,
					addressedCommentIds: addressed,
					submittedCommentCount: pending.commentIds.size,
				},
			};
		},
	});

	pi.registerCommand("leaf-review", {
		description: "Open a Markdown file in a connected Leaf review session",
		handler: async (args, ctx) => {
			const rawPath = args.trim().replace(/^@/, "");
			if (!rawPath) {
				ctx.ui.notify("Usage: /leaf-review <file.md>", "warning");
				return;
			}
			const path = normalizeToolPath(rawPath, ctx.cwd);
			if (!isMarkdownPath(path) || fileFingerprint(path) === null) {
				ctx.ui.notify(`Not a readable Markdown file: ${path}`, "error");
				return;
			}
			ensureReview(path, ctx);
		},
	});

	pi.on("session_start", (_event, ctx) => {
		currentContext = ctx;
		const sessionId = ctx.sessionManager
			.getSessionId()
			.replace(/[^a-zA-Z0-9_-]/g, "-")
			.slice(0, 48);
		sessionRoot = join(tmpdir(), `leaf-pi-${sessionId}-${process.pid}-${Date.now()}`);
		mkdirSync(sessionRoot, { recursive: true, mode: 0o700 });
		if (pollTimer) clearInterval(pollTimer);
		pollTimer = setInterval(pollRequests, POLL_INTERVAL_MS);
		pollTimer.unref();
	});

	pi.on("tool_call", (event, ctx) => {
		let paths: TrackedPath[] = [];
		if (isToolCallEventType("write", event) || isToolCallEventType("edit", event)) {
			const path = normalizeToolPath(event.input.path, ctx.cwd);
			if (isMarkdownPath(path)) {
				paths = [{ path, before: fileFingerprint(path), alwaysReview: true }];
			}
		} else if (isToolCallEventType("bash", event)) {
			paths = shellMarkdownPaths(event.input.command, ctx.cwd).map((path) => ({
				path,
				before: fileFingerprint(path),
				alwaysReview: false,
			}));
		} else if (event.toolName === "powershell") {
			const command = (event.input as { command?: unknown }).command;
			if (typeof command === "string") {
				paths = shellMarkdownPaths(command, ctx.cwd).map((path) => ({
					path,
					before: fileFingerprint(path),
					alwaysReview: false,
				}));
			}
		}
		if (paths.length > 0) trackedTools.set(event.toolCallId, paths);
	});

	pi.on("tool_execution_end", (event) => {
		const tracked = trackedTools.get(event.toolCallId);
		trackedTools.delete(event.toolCallId);
		if (event.isError || !tracked) return;
		for (const candidate of tracked) {
			const after = fileFingerprint(candidate.path);
			if (after && (candidate.alwaysReview || after !== candidate.before)) {
				changedMarkdown.add(canonicalPath(candidate.path));
			}
		}
	});

	pi.on("agent_settled", (_event, ctx) => {
		for (const requestId of [...pendingReviews.keys()]) {
			failReview(
				requestId,
				"Pi finished without confirming which comments were addressed; the comments are ready to resend",
				ctx,
			);
		}

		const documents = [...changedMarkdown];
		changedMarkdown.clear();
		for (const documentPath of documents) {
			if (fileFingerprint(documentPath) !== null) ensureReview(documentPath, ctx);
		}
	});

	pi.on("session_shutdown", () => {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = undefined;
		}
		for (const channel of channels.values()) {
			try {
				writeEvent(channel, {
					type: "bridge_closed",
					message: "The Pi session ended; Leaf was left open",
				});
			} catch {
				// The application is deliberately not terminated on disconnect.
			}
		}
		currentContext = undefined;
	});
}
