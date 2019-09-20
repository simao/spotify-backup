all: spotify-backup.docker.tar.zst

db:
	sqlite3 data/backup-requests.db < schema.sql

unit-test:
	cargo test -- --skip full_backup

target/release/spotify_backup: src/
	cargo build --release

spotify-backup.docker.tar.zst: Dockerfile target/release/spotify_backup
	docker build . -t simao/spotify-backup:latest
	docker save simao/spotify-backup:latest | zstdmt > spotify-backup.docker.tar.zst

push: spotify-backup.docker.tar.zst
	rsync -a --progress spotify-backup.docker.tar.zst docker-compose.yml sm-data@0io.eu:spotify-backup/
e	rsync -a --progress spotify-backup.example.toml sm-data@0io.eu:spotify-backup/spotify-backup.toml

install: push
	ssh simao@0io.eu "zstdcat /home/sm-data/spotify-backup/spotify-backup.docker.tar.zst | docker image load"

docker-run: docker
	docker run -it --publish 8000:8000 --volume $(pwd):/tmp/spotify-backup -e DATA_DIR=/tmp/spotify-backup -e RUST_LOG=info,spotify_backup=debug simao/spotify-backup:latest server

clean:
	rm spotify-backup.docker.tar.zst

.PHONY: docker docker-run install push
