# Restic Binaries for Testing

This directory contains platform-specific restic binaries used for integration testing.

## Current Binaries

- `restic-linux` - Restic binary for Linux (x86_64)
- `restic-windows.exe` - Restic binary for Windows (x86_64)

## Platform Detection

The test suite automatically selects the appropriate binary based on the target platform:

- Linux/Unix: Uses `restic-linux`
- Windows: Uses `restic-windows.exe`

This is handled by the `get_restic_binary_name()` function in `tests/restic_integration_tests.rs`.

## Binary Sources

These binaries are used solely for testing and are not distributed with the production build. Users must install restic separately on their systems.

- Restic Homepage: https://restic.net/
- Restic GitHub: https://github.com/restic/restic
