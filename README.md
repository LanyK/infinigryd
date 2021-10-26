# Infinite Grid of Instances

## Project Description

This project consists of several parts:

- A generic actor model implementation in rust: `actlib`
- A networking crate written in rust: `netchannel`
- A pacman-inspired game mockup `ìnfinigryd` with an infinite board. Is uses `actlib` and is written in rust as well.
- A javascript web-application displaying the ``infinigryd` game

### actlib: Actor Model library implementation

The actor model is a model for concurrent computation, a short introduction can be found here:

[https://www.brianstorti.com/the-actor-model/](https://www.brianstorti.com/the-actor-model/)

Actors are isolated computing units with an internal state and the ability to send messages to other locally known actors as well as spawn new actors on demand.

![Actor Model Example Image](https://www.brianstorti.com/assets/images/actors.png)

Our library will provide a public API to define actor variants and their behavior, and define the kinds of messages they can send and receive via easy-to-implement yet powerful traits and predefined helper classes.

The library provides further abstraction where the `Environment` that holds the actors can be seamlessly distributed along multiple machines without changing the API usage or view on the data for the executing main program. The Environments running on different machines communicate transparently to the Actors using a TCP connection (using `netchannel`).

### Netchannels

`netchannels` emulate the channel-Interface for communication between threads.

They are implemented using a TCP-Connection and are used for every remote connection in `actlib`.

### Infinite Grid Application (Infinigryd)

Our 'pacman' inspired game simulation `ìnfinigryd` will have multiple actor instances connected to each other in the form of a 2D grid.
The 'player' instances inhabit cells in that grid and can move between cells - that is, when one 'player' leaves the current field in a certain direction, he will reappear from the opposite direction on the neighboring grid.
This is inspired from the original pacman game's wrapping playing field border functionally - thus the name, although in this case the playing field does not wrap, but a new neighboring field is entered.

Since only grid cells that are currently inhabited need to be alive, the actor model allows for a dynamically simulated de-facto infinite grid:
Our simulated 'players' will move pseudo-randomly between grid cells and the corresponding actors can spawn new neighboring actors on demand or shut themselves down when the last inhabitant has left them.
Due to time constraints the 'player' don't actually move on the playing field, but simply leave it in a random direction after some delay.

In our implementation each playing field instance will be represented by an actor. All actors will run distributed on the provided servers and a supervising master thread will run on a third laptop/computer. The goal of the actlib framework is to both allow the instances can communicate with each other no matter on which server they are, as well as to provide a single view for the master program 'as if' all actors were run locally.

### Browser View for the Infinite Grid Application

The web application renders the game logic to inspect and verify the behavior. 
Every second it receives a status update from one of the remote servers with all currently active actors (game fields). 
If an actor is hosted on "agakauitai", the field is displayed in green, if the actor is hosted on "haruku", the field is displayed in red. 
Additionally the amount of players on each field is displayed. 

## How to build

In the toplevel folder run
`cargo build` or `cargo build --release`.
This has to be done on all machines.

If you are using a proper Unix-like operating system you can use the provided shellscript:
`./push.sh`
This will first build everything locally and then push the relevant files using `scp` to the hosts `agakauitai` and `haruku`.
You will probably need to adjust your `~/.ssh/config` to include them for it to work.
An example using 'username' as login name is shown below.

`.ssh/config`
```
Host agakauitai
  Hostname 141.84.94.111
  Port     54321
  User     username
Host haruku
  Hostname 141.84.94.207
  Port     54321
  User     username
```

To configure the view for displaying the results in your browser move the webworker application in  `./target/release/` into `./view/www/` and change the name to data.json. 
Additionally modify the `THTTPD_ROOT` path directory in `view/bin/thttpd.start` to the path of the view folder of your system.

## How to run

### infinigryd

The example program expects a `machines.cfg` either in the current working directory, or its subfolder cfg.
If you cloned the git repository or used the shell script it should be in the expected location.
You can recreate it by using the command `cargo run --bin cfg-generator`.

The `infinigryd` example program has to be started on all machines.
On your local machine run the `thttpd.start` in the `view/bin` folder and go on `localhost:8080` in a browser in order to view the results of the example program.

When using the shell script you have to start in on the remote servers using `~/infinigryd`.

### work-distributer-test

`work-distributer-test`` is a simple proof-of-concept application building a tree of actors distributing work among them.

## Contributions

A lot done as joint work on large screen

Individual **emphases**:
- Philipp Ammann: Netchannel, Frontend
- Yannick Kaiser: Library, work-distributer-test
- Korbinian Staudacher: Netchannel, Collecting Actor
- Wolfgang Witt: infinigryd example application, Library

## Group Members

- Philipp Ammann
- Yannick Kaiser
- Korbinian Staudacher
- Wolfgang Witt
