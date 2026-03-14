#!/usr/bin/env node

/**
 * Quarto Hub MCP Server
 *
 * An MCP server that provides AI coding agents with direct access
 * to Quarto Hub projects via automerge sync. Agents can read and write
 * files in collaborative projects without filesystem access.
 *
 * Usage:
 *   quarto-hub-mcp --server https://hub.example.com
 *   quarto-hub-mcp --server https://hub.example.com --read-only
 *
 * Environment variables:
 *   QUARTO_HUB_SERVER - Sync server URL (overridden by --server)
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { ConnectionManager } from './connection-manager.js';
import { registerTools } from './tools.js';

function parseArgs(argv: string[]): { serverUrl: string; readOnly: boolean } {
  let serverUrl = process.env['QUARTO_HUB_SERVER'] ?? '';
  let readOnly = false;

  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--server' && i + 1 < argv.length) {
      serverUrl = argv[++i]!;
    } else if (arg === '--read-only') {
      readOnly = true;
    } else if (arg === '--help' || arg === '-h') {
      console.error(`Usage: quarto-hub-mcp --server <url> [--read-only]

Options:
  --server <url>   Automerge sync server URL (or set QUARTO_HUB_SERVER)
  --read-only      Only expose read tools (no write/create/delete)
  --help, -h       Show this help message`);
      process.exit(0);
    } else {
      console.error(`Unknown argument: ${arg}`);
      process.exit(1);
    }
  }

  if (!serverUrl) {
    console.error('Error: --server <url> or QUARTO_HUB_SERVER is required');
    process.exit(1);
  }

  return { serverUrl, readOnly };
}

async function main(): Promise<void> {
  const { serverUrl, readOnly } = parseArgs(process.argv);

  const manager = new ConnectionManager(serverUrl);

  const server = new Server(
    {
      name: 'quarto-hub',
      version: '0.0.1',
    },
    {
      capabilities: {
        tools: {},
      },
    }
  );

  registerTools(server, manager, readOnly);

  const transport = new StdioServerTransport();
  await server.connect(transport);

  // Clean up on exit
  process.on('SIGINT', async () => {
    await manager.disconnectAll();
    await server.close();
    process.exit(0);
  });
  process.on('SIGTERM', async () => {
    await manager.disconnectAll();
    await server.close();
    process.exit(0);
  });
}

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
