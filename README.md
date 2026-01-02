# Steamer

## What's this and why?
Steamer finds your installed Steam games and creates shortcuts for them in your application menu. This is particularly useful if you're migrating from Windows to Linux and have a library of Steam games on a separate drives and want your nice start menu/application launcher shortcuts back.

## Techy stuff

Steam stores the current Steam library paths in a file called `libraryfolders.vdf`. From that Steamer extracts the paths to the actual libraries, and checks for installed games in the `steamapps` folder which contains a list of `*.acf` files. These files contain metadata about each installed game, including its name, installation path, and app ID. Steamer reads these files to gather information about your installed games and then generates a `.desktop` for each one it finds (skipping tools such as Proton and the various Linux runtimes).

## How to install
If you have Cargo installed, you can install Steamer by running:

```bash
cargo install steamer
```

## How to build from source
1. Install Rust and Cargo. 
2. Open a terminal in this folder.
3. Run the build command:
   ```bash
   cargo build --release
   ```
4. The executable file will be created at `target/release/steamer`.
