# Spotify Backup Tool

Tool to backup your spotify albums and playlists into a single json file. This is useful if you just want to backup your data or want to run some analysis on the songs you have on your library.

This app can run in two modes:

1. A CLI tool that prints your spotify data to stdout

2. A web app that accepts that redirects the user to spotify to get authorization and then provides the user with a json backup of the data.

A working instance of this app is running at [0io.eu/spotify-backup](https://0io.eu/spotify-backup/).

## Running the CLI tool

You'll need to create an application with [Spotify](https://developer.spotify.com/). You'll need to add `http://localhost:8000` as a callback url. Get a client id and client secret and then run this app with:
    
    CLIENT_ID=<client id> CLIENT_SECRET=<client_secret> RUST_LOG=info,spotify_backup=debug cargo run > backup.json
    
Follow the instructions to allow the app to access your data. You can then delete your Spotify App your just unauthorize your user.

## Running the web tool

You'll need to run both the frontend and backend app. You can have a `env` file that exports `CLIENT_ID` and `CLIENT_SECRET`. You can also change some settings like the base uri used by your app, setup in the [dev console](https://developer.spotify.com/), in `spotify-backup.toml`, there is an example file in the root directory. 

    source env
    RUST_LOG=info,spotify_backup=info cargo -- server
    
In a separate process run:

    source env
    RUST_LOG=info,spotify_backup=info cargo -- worker

A Dockerfile and a docker-compose file are provided but you'll need to build your own images.
