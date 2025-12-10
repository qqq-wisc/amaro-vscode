import * as path from 'path';
import * as fs from 'fs';
import { workspace, ExtensionContext, window } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
	console.log("Activating Marol Extension...");

	const binaryName = process.platform === 'win32' ? 'marol-lsp.exe' : 'marol-lsp';
	const serverPath = context.asAbsolutePath(
		path.join('marol-lsp', 'target', 'debug', binaryName)
	);
	console.log(`Looking for LSP binary at: ${serverPath}`);

	if (!fs.existsSync(serverPath)) {
		window.showErrorMessage(`Marol LSP binary not found! Expected at: ${serverPath}. Did you run 'cargo build'?`);
		console.error(`Binary missing at ${serverPath}`);
		return;
	}

	if (process.platform !== 'win32') {
		fs.chmodSync(serverPath, '755');
	}

	const serverOptions: ServerOptions = {
		run: { command: serverPath, transport: TransportKind.stdio },
		debug: { command: serverPath, transport: TransportKind.stdio }
	};

	const clientOptions: LanguageClientOptions = {
		documentSelector: [{ scheme: 'file', language: 'marol' }],
	};

	client = new LanguageClient(
		'marolLSP',
		'Marol Language Server',
		serverOptions,
		clientOptions
	);

	client.start().then(() => {
		console.log("Marol LSP Started!");
		client.outputChannel.show(true);
	}).catch(err => {
		console.error("Marol LSP Failed to start:", err);
	});
}

// This method is called when your extension is deactivated
export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
