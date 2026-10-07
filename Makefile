.PHONY: setup acpd start pair android android-test test probe chat
setup:
	cargo fetch --locked
acpd:
	cargo build --locked --workspace --bins
start:
	cargo run --locked -p acpd -- --config examples/config.toml start
pair:
	cargo run --locked -p acpd -- --config examples/config.toml pair
android:
	cd android/universal-acp && ./gradlew :app:assembleDebug
android-test:
	cd android/universal-acp && ./gradlew :core:protocol:test :app:testDebugUnitTest
test:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace --all-targets
probe:
	cargo run --locked -p acpd -- --config examples/config.toml probe --workspace .
chat:
	cargo run --locked -p acpd -- --config examples/config.toml chat --workspace .
