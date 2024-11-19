# reka

Custom [Backstage](https://backstage.io/) Entity Provider (BEP) for Kubernetes. Based on filtering rules BEP starts watching desired k8s resources and creates various Backstage Entities exposed over `/api/v1/entities` HTTP endpoint.

# TODO
- test processing of kube_runtime::watcher::Event enums that changed with the upgrade. 
- test deployment into Kind k8s.
- test auto-discovery of various k8s workloads.
- improve instructions.