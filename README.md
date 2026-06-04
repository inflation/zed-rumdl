# Rumdl Zed Extension

Zed extension for the
[Rumdl Markdown Linter](https://github.com/rvben/rumdl).

## Supported platforms

| Platform | X86_64 | ARM64 |
| -------- | ------ | ----- |
| Linux    | ✅     | ✅    |
| macOS    | ✅     | ✅    |
| Windows  | ✅     | ❌    |

## Install

1. [Open the Extension Gallery](https://zed.dev/docs/extensions/installing-extensions)
2. Search for `Rumdl` in the Gallery
3. Click "Install"!

## Configuration

You do not need to install `rumdl` separately. If it is not already on your
`PATH`, the extension downloads a release binary automatically and keeps it up
to date. To use a specific binary instead, set a path in your Zed settings:

```json
{
  "lsp": {
    "rumdl": {
      "binary": {
        "path": "/path/to/rumdl"
      }
    }
  }
}
```

Binary resolution order:

1. A configured `binary.path` (if set)
2. `rumdl` found on your `PATH`
3. Otherwise, a release binary downloaded automatically from GitHub
   (no manual install needed)
