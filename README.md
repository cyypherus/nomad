<div align="center">

# nomad

**A nomad network micron browser tui**

</div>
<img width="100%" alt="Screenshot 2026-01-30 at 6 12 43 PM" src="https://github.com/user-attachments/assets/670f119f-20f3-4d30-b3bd-e078f105f5bb" />

# Getting Started

- The first time the tui runs, it will create a hidden directory with some persistent state at the current working directory `.rinse/`.

- Inside the `.rinse` directory you will find a `config.toml` with some instructions for finding and setting up interfaces.

- Once you've set up interfaces in your config file you can rerun the application and wait for incoming announces on the network.

- New announces on the network will show up in the Discovery tab, you can save discovered nodes for future reconnection, & they'll show up in the Saved tab.

- Clicking connect on a node from the discovery or the saved tabs will attempt to fetch the default page for the selected node.

- If the node responds you'll see an interactive micron webpage rendered. You're officially surfing nomadnet using reticulum mesh networking.

> [!NOTE]
> `nomad`, [micronaut](https://github.com/cyypherus/micronaut), [rinse](https://github.com/cyypherus/rinse) and `reticulum` in general are all relatively young software. You'll probably run into some bugs here and there.

# Contributing

By submitting a contribution, you agree that it will be licensed under the project’s license.
