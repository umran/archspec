# conseqa-viz front end

React + TypeScript + Vite, styled with Tailwind CSS v4 and Cloudflare's
Kumo design system. Consumes the page data `conseqa-viz` produces —
`window.CONSEQA` in the embedded build, `public/conseqa.json` in
development.

```
npm install
npm run data     # regenerate public/conseqa.json from the video-streaming example
npm run dev      # http://localhost:5173
npm run build    # typecheck + single-file bundle → dist/index.html
```

`dist/index.html` is committed: the Rust binary embeds it with
`include_str!`, so rebuild and commit it after changing the front end.
See `CONSEQA_VIZ.md` at the repository root for the views, panels, and
report format.

`App` also takes an optional `theme` prop. Without it the app owns the
colour mode — restoring the stored choice, setting `data-mode`, and
offering a toggle — which is what the embedded build needs. With it,
the host owns the mode and the app neither persists it nor shows a
toggle, so an application embedding these views has exactly one
control for it.
