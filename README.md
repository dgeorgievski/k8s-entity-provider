# k8s-entity-provider

Custom [Backstage](https://backstage.io/) Entity Provider (BEP) for Kubernetes. Based on filtering rules BEP starts watching desired k8s resources and creates various Backstage Entities exposed over `/api/v1/entities` HTTP endpoint.

## TODO 

- test deployment into Kind k8s.
- test auto-discovery of various k8s workloads.
- improve instructions.
