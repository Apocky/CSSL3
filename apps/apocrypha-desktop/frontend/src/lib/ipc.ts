// The only bridge to the application. Each call returns the whole view, so the
// window never has to guess what changed — and never holds a token to begin
// with, because the session lives in the Rust process.

import { invoke } from '@tauri-apps/api/core';
import type { View } from './view.ts';

async function call(command: string, args?: Record<string, unknown>): Promise<View> {
  return invoke<View>(command, args);
}

export const ipc = {
  bootstrap: () => call('bootstrap'),
  sendCode: (email: string) => call('send_code', { email }),
  createAccount: (email: string) => call('create_account', { email }),
  createWithPassword: (email: string, password: string) => call('create_with_password', { email, password }),
  signIn: (email: string, secret: string, password: boolean) => call('sign_in', { email, secret, password }),
  refresh: () => call('refresh'),
  openConversation: (id: string) => call('open_conversation', { id }),
  newConversation: () => call('new_conversation'),
  send: (text: string) => call('send', { text }),
  signOut: () => call('sign_out'),
};
