# Starting ThinkCraft

## Download a ready-made build
Each [GitHub release](https://github.com/Simsen/ThinkCraft/releases) has:
- `thinkcraft-windows-x64.zip`: unzip it and double-click **ThinkCraft.exe**.
- `thinkcraft-macos-arm64.zip`: unzip it and drag **ThinkCraft.app** into Applications.

Builds of the latest code are also under [Actions](https://github.com/Simsen/ThinkCraft/actions) → the newest **Build** run → *Artifacts* (you need to be signed in to GitHub).

The builds aren't signed with a paid certificate yet, so the first time you start one:
- **Windows:** if SmartScreen says "Windows protected your PC", click **More info** → **Run anyway**.
- **macOS:** right-click the app and choose **Open** once.

## Windows: build it yourself
1. Install Rust from [rustup.rs](https://rustup.rs). When asked, let it install the Visual Studio C++ build tools.
2. In PowerShell, from this folder:

       cargo build --release

3. Start `target\release\thinkcraft.exe`. You can copy it anywhere, for example onto your Desktop; it's a single file.

## macOS: build it yourself
1. Install Rust from [rustup.rs](https://rustup.rs).
2. In Terminal, from this folder:

       sh make-mac-app.sh

3. Double-click `dist/ThinkCraft.app`, or drag it into your **Applications** folder so it shows up in Launchpad and Spotlight.
   If you dragged an older copy into Applications, drag the new one there again and choose Replace.

## Keyboard shortcuts
On Windows, ThinkCraft uses Ctrl where a Mac uses Cmd (Ctrl+C, Ctrl+V, Ctrl+Z, Ctrl+S…),
and Alt where a Mac uses Option (hold Alt to move things without snapping).

## Where your work is kept
- Decks you save yourself: wherever you choose (`.tcraft` files).
- The last open deck is also kept automatically in
  - Windows: `%APPDATA%\ThinkCraft\`
  - macOS: `~/Library/Application Support/ThinkCraft/`
