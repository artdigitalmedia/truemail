# TrueMail: building and installing this fork

TrueMail is a fork of MailCraft that adds two buttons beside Reply: **Canned text** and
**Draft a reply**. This page is for whoever sets it up on a computer. The fork has no installer
of its own yet, so it is built from source. Everything else about MailCraft is in `README.md`.

## What the fork adds

- **Canned text.** Replies you keep by name. Picking one starts a reply with that text above
  your signature, for you to change or send. Each can say what becomes of the message answered
  once the reply is sent: left where it is, deleted, archived or moved to a folder.
- **Draft a reply.** A language model on the computer writes a reply to the open message, which
  opens for you to change or send. Nothing is sent for you, and the message goes nowhere else.

Both are on the `reply-buttons` branch. `main` follows MailCraft unchanged.

## Build it (Windows)

1. Install [Git for Windows](https://git-scm.com/download/win).
2. Install Rust from [rustup.rs](https://rustup.rs). If its installer asks for the Visual Studio
   C++ Build Tools, let it install them. The build needs the Rust version in `Cargo.toml`
   (`rust-version`) or newer.
3. In PowerShell, with your fork's address in place of `OWNER`:

   ```powershell
   git clone -b reply-buttons https://github.com/OWNER/truemail C:\TrueMail
   cd C:\TrueMail
   cargo build --release -p mailcraft
   ```

The program is then `C:\TrueMail\target\release\mailcraft.exe`.

To update later, close the program (Windows can't replace it while it runs), then:

```powershell
cd C:\TrueMail
git pull
cargo build --release -p mailcraft
```

## Where it keeps things

- Mail and settings are in MailCraft's data folder (`%APPDATA%\MailCraft` on Windows), which
  the released MailCraft uses too. Don't run both at once on one computer.
- Canned texts are settings: they are in `ui.json` in that folder.
- Passwords and sign-ins are in the system's own credential store, so accounts are added again
  on each computer.

## Sign in with Microsoft

A build made this way carries no registered app, so an Outlook.com account can't be added until
it has one. Register an app as `docs/oauth.md` describes, then give its client ID to the program
once per computer:

```powershell
setx MAILCRAFT_MICROSOFT_CLIENT_ID "your-application-client-id"
```

Start the program from a new window afterwards. The ID is read each time it starts, so a rebuild
doesn't lose it.

Two things `docs/oauth.md` doesn't say, found while doing this:

- A personal Microsoft account can no longer register an app on its own. It needs a directory
  first, which signing up for Azure creates, or which it may already have from another Microsoft
  service. Register the app in a directory that is yours.
- The app signs in through `http://127.0.0.1`, so that redirect URI must be registered, under
  **Mobile and desktop applications**. `http://localhost` alone is not enough. If the portal
  refuses the address, add it in the app's manifest.

Keep your own client ID and directory details out of this repository: they belong in your
private notes.

## The model for Draft a reply

Draft a reply runs llama.cpp's `llama-server` for the one draft and stops it afterwards. It needs
that program and a model file (a `.gguf`). With none chosen, it uses the ones TrueVoice (a
dictation program) keeps:

| What | Where it's looked for |
|---|---|
| The program | `%LOCALAPPDATA%\TrueVoice\engines\llama\llama-server.exe` |
| The model | the first `.gguf` file, by name, in `%APPDATA%\TrueVoice\models` |

On a computer without TrueVoice, either install it or copy the `llama` folder (all of it: the
program needs the files beside it) and a model file to those places. Others can be chosen with
the `assistant.set` command (`mailcraft-cli commands` lists its parameters).

When either is missing, the button says which in a message at the bottom of the window. Canned
text needs none of this.
