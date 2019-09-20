FROM debian:stable-slim

RUN apt-get update && apt-get upgrade --yes && apt-get install --yes libsqlite3-dev libssl-dev ca-certificates

WORKDIR /opt/spotify-backup

ENV PATH=${PATH}:/opt/spotify-backup

COPY target/release/spotify_backup /opt/spotify-backup/spotify-backup

COPY static /opt/spotify-backup/static
COPY templates /opt/spotify-backup/templates

EXPOSE 8000

CMD ["/opt/spotify-backup/spotify-backup"]
