# S1-C account fixture

This is a standalone, offline browser fixture for the account, Friends, and Share-list screens. It uses a deterministic in-memory fake controller. It does not call a service, open a real sign-in flow, persist data, or connect to the desktop shell.

## Launch

From the `apps/desktop/src` directory in this worktree, run:

```powershell
py -3 -m http.server 4173 --bind 127.0.0.1
```

Open `http://127.0.0.1:4173/account/s1c/` in a browser. The local static server is needed because browser modules are restricted under `file://`; the page and its assets remain on loopback. Stop the server with Ctrl+C.

The frozen renderer test needs no server or installed JavaScript packages:

```powershell
node --test apps/desktop/tests/account-s1c.test.ts
```

Use the state chooser to view all ten account states. The navigation opens Account, Friends, and Share list. Turning on “เปิดบริการจำลองในเครื่อง” enables only the fake local interactions.
