# GitHub Pages

This directory is a buildless static site: it uses only `index.html`,
`guia.html`, `styles.css`, and `app.js`.

`index.html` presents Lia and its executable examples. `guia.html` is a
step-by-step Portuguese tutorial covering syntax, functions, routines, loops,
signals, imports, static analysis, compilation, and runtime execution.

To publish it with GitHub Pages, open the repository **Settings → Pages**,
select **Deploy from a branch**, choose the branch containing this folder, and
set the source folder to **`/docs`**. No build command is required.

## Language specifications

The [specification index](especificacoes/README.md) collects the language models
and feature explanations. The project [TODO](../TODO.md) records implementation
issues and a separate documentation/commenting backlog.
