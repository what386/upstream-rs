# Registry News

This file records registry changes that may require user attention or manual action.

Entries are reserved for:

- Removed, renamed, or replaced packages
- Incorrect or compromised package definitions
- Changes to package trust requirements
- Registry schema compatibility changes
- Temporary installation warnings
- Changes that require users to refresh the registry or reinstall a package

Routine package additions, metadata corrections, and revision increments do not need an entry. Those changes remain visible in package history and the generated index.

## Entry format

Add new entries above older entries using this format:

```markdown
## YYYY-MM-DD — Short description

Severity: info | warning | critical

Affected packages: package-name, another-package

Describe the change, its impact, and any required commands or manual steps.
```

Omit `Affected packages` when an entry applies to the registry as a whole.

There are currently no registry notices.
