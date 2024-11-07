TAG?=latest
NAME:=axyom-backstage-provider
DOCKER_REPOSITORY:=registry.gitlab.casa-systems.com/dimitar.georgievski
DOCKER_IMAGE_NAME:=$(DOCKER_REPOSITORY)/$(NAME)
DOCKERFILE?=docker/Dockerfile
GIT_COMMIT:=$(shell git describe --dirty --always)
VERSION:=$(shell /usr/local/bin/toml get Cargo.toml package.version | tr -d '"')
EXTRA_RUN_ARGS?=
DEPLOY_ENV?=prod

build:
	cargo build 

build-release:
	cargo build --locked --release

run:
	cargo run 

test:
	cargo test

docker-build:
	cargo check
	docker build -t $(DOCKER_IMAGE_NAME):$(VERSION) -f $(DOCKERFILE) .

docker-push:
	docker push $(DOCKER_IMAGE_NAME):$(VERSION)

version-get:
	@echo "Current version $(VERSION)"

version-set:
	@next="$(TAG)" && \
	current="$(VERSION)" && \
	/usr/bin/sed -i "s/^version = \"$$current\"/version = \"$$next\"/g" Cargo.toml && \
	/usr/bin/sed -i "s/provider\:$$current/provider\:$$next/g" deploy/kpt/$(DEPLOY_ENV)/axyom-backstage-provider/deployment.yaml && \
	echo "Version $$next set in code and deploy/kpt/$(DEPLOY_ENV)/axyom-backstage-provider manifests"

deploy:
	@olddir=`pwd` 
	cd deploy/kustomize 
	kubectl apply -k overlays/cicd/
	cd $$olddir