# Test Fixtures

This directory stores reusable fixture files for tests.

Rules:
- Keep fixtures small and deterministic.
- Prefer behavior-driven filenames over one-file scenario directories.
- Use scenario directories only when multiple files must keep their original
  names together or when directory layout is part of the behavior under test.
- Avoid adding large binaries directly when generation is practical.
- When adding generated fixtures, document regeneration commands in the nearest README.
