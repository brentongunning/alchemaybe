-include .env
export

MODEL ?= $(OLLAMA_MODEL)
IMAGE_MODEL ?= $(OLLAMA_IMAGE_MODEL)

.PHONY: setup run

setup:
	@which ollama > /dev/null || (echo "Installing Ollama..." && brew install ollama)
	ollama pull $(MODEL)
	ollama pull $(IMAGE_MODEL)

run:
	cargo build -p generation -p game
	trap 'kill 0' EXIT; cargo run -p generation & cargo run -p game & wait
