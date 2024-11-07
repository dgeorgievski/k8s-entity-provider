# ci-cd

## Description
sample description

## Usage

### Fetch the package
`kpt pkg get REPO_URI[.git]/PKG_PATH[@VERSION] ci-cd`
Details: https://kpt.dev/reference/cli/pkg/get/

### View package content
`kpt pkg tree ci-cd`
Details: https://kpt.dev/reference/cli/pkg/tree/

### Apply the package
```
kpt live init ci-cd
kpt live apply ci-cd --reconcile-timeout=2m --output=table
```
Details: https://kpt.dev/reference/cli/live/
