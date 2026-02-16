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
	console.log("Activating Amaro Extension...");

	const binaryName = process.platform === 'win32' ? 'amaro-lsp.exe' : 'amaro-lsp';
	const serverPath = context.asAbsolutePath(
		path.join('amaro-lsp', 'target', 'release', binaryName)
	);
	console.log(`Looking for LSP binary at: ${serverPath}`);

	if (!fs.existsSync(serverPath)) {
		window.showErrorMessage(`Amaro LSP binary not found! Expected at: ${serverPath}. Did you run 'cargo build'?`);
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
		documentSelector: [{ scheme: 'file', language: 'amaro' }],
	};

	client = new LanguageClient(
		'amaroLSP',
		'Amaro Language Server',
		serverOptions,
		clientOptions
	);

	client.start().then(() => {
		console.log("Amaro LSP Started!");
		client.outputChannel.show(true);
	}).catch(err => {
		console.error("Amaro LSP Failed to start:", err);
	});
}

// This method is called when your extension is deactivated
export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
