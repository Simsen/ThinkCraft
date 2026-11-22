# Starting ThinkCraft on its own

## The quick way
1. Open Finder and go to this project folder, then `app` → `dist`.
2. Double-click **ThinkCraft.app**.
3. To keep it handy, drag **ThinkCraft.app** into your **Applications** folder.
   It then shows up in Launchpad and Spotlight (press Cmd+Space, type "ThinkCraft").
   You can also right-click its Dock icon while it's running → Options → Keep in Dock.

If macOS ever says it can't check the app, right-click it and choose **Open** once.

## After changes to the app
The `dist/ThinkCraft.app` copy is rebuilt with:

    sh make-mac-app.sh

Run it in Terminal from this `app` folder. If you dragged an older copy into Applications,
drag the new one there again and choose Replace.

## Where your work is kept
- Decks you save yourself: wherever you choose (`.tcraft` files).
- The last open deck is also kept automatically in
  `~/Library/Application Support/ThinkCraft/`.
