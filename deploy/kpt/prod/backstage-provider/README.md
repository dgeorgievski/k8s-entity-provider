# acme-backstage-provider

## Description
sample description

## Usage

### Fetch the package
`kpt pkg get REPO_URI[.git]/PKG_PATH[@VERSION] acme-backstage-provider`
Details: https://kpt.dev/reference/cli/pkg/get/

### View package content
`kpt pkg tree acme-backstage-provider`
Details: https://kpt.dev/reference/cli/pkg/tree/

### Apply the package
```
kpt live init acme-backstage-provider
kpt live apply acme-backstage-provider --reconcile-timeout=2m --output=table
```
Details: https://kpt.dev/reference/cli/live/
