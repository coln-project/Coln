# Coln NixOS Runner
A NixOS configuration for a self-hosted github server for the Coln repo. The configuration supports spawning $n$ workers.

# Setup
The configuration uses `agenix` to provision secrets. The configuration provision s the following:

- *PAT Token* : For github. Provides read/write access to the Coln repository and the ability to run actions. Stored in `runner-token.age`.
- *Wifi Passwords* : Wifi is managed declaratively, with wifi ssid/password in `wifi.age`
- **[OPTIONAL]** *Root user password* : Set in `root-pass.age`
- **[OPTIONAL]** *Root user ssh key* : Set in `root-user-ssh.age`

The runner won't be functional without the first two of these.

## `agenix` Setup
Additionally before we can even think about secrets we need to make sure that agenix can decrypt them on the target. This requires knowing both:

- *Host user ssh public key* (`~/.ssh/<key>.pub`) : So that we can read/write secrets on the host.
- *Target host ssh public key* (`/etc/ssh/ssh_host_ed25519_key.pub`) : So that the target can decrypt secrets on boot.

Once knowing these we can add these public keys to `secrets.nix` so that they can read/written to by those users:

```nix
let

  # users
  host-user = "<host-user-key-here>";
  users = [host-user];

  # systems
  target-system = "<target-key-here>";
  systems = [target-system];

  all = users ++ systems;

in
{
  ...
}
```

Which will then allow us to create files via `agenix -e <filename>`.

## PAT Token
After generating a classic github PAT token via [this](https://github.com/settings/tokens) link and entitling it to repo permissions (only this is necessary), the token can be encrypted to a file via `agenix -e runner-token.age` in the `secrets` directory. Paste the token into the file.

## Wifi Passwords
Wifi management is declarative. Adding a network consists of two parts:

1. Adding the SSID to the expression in `networking.wireless.networks` in `default.nix`:
  ```nix
      networks = builtins.listToAttrs
        (map (name: { inherit name; value.pskRaw = "ext:PSK_${name}"; })
            ["<YOUR SSID HERE>"]);
  ```
2. Adding the SSID:Password pair to `secrets/wifi.age`. Run `agenix -e wifi.age` in `secrets` and add a single line:
  ```txt
  PSK_<SSID>=<Password>
  ```
  So a network with SSID `mynetwork` and password `mypassword` would correspond to a line:
  ```txt
  PSK_mynetwork=mypassword
  ```

## Root user
Additionally the machine currently sets up only one user, the `root` user. The password and user ssh keys for this are managed declaratively.

### Root user password
If you wish to declaratively configure a root password you can do so by generating a hashed password via `mkpasswd`:
```sh
mkpasswd -m sha-512 | wl-copy # generate a hashed password
cd secrets
agenix -e root-pass.age # paste hashed password
```
### Root ssh key
Additionally if you want to provision the root user with an ssh key you can do so by generating a pair of keys and copying the following files:

- *Public Key* to `secrets/root-user-ssh.pub`
- *Private Key* to `secrets/root-user-ssh.age` via `agenix -e root-user-ssh.age`

# Execution
> [!NOTE]
> Text of the form `$<variable>` points to a configurable variable in `github.nix`

The configuration spawns $n =$`$workers` runners on startup. These are each run in their own directory under the root `$dirRoot`, with name `coln-runner-<x>` where $x \in \{1, \cdots , n\}$. Each of these runners share a cache directory `.cache` living under `$dirRoot`. Cargo build artefacts are stored here.

```txt
dirRoot/
├── cache/
│   ├── .cache/
│   │   └── nix/          (XDG_CACHE_HOME - nix cache)
│   ├── cargo-home/        (CARGO_HOME - crate registry/downloads)
│   └── cargo-target/      (CARGO_TARGET_DIR - compiled artifacts)
├── coln-runner-1/
│   └── (workDir - job checkout and execution)
├── coln-runner-2/
│   └── (workDir - job checkout and execution)
├── coln-runner-3/
│   └── (workDir - job checkout and execution)
│   ...
└── coln-runner-N/
    └── (workDir - job checkout and execution)
```

## Configurable Variables
- `workers` : The number of runners to spawn
- `rootDir` : The directory in which to store cache/workers

