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
  "root-pass.age".publicKeys = all;
  "runner-token.age".publicKeys = all;
  "wifi.age".publicKeys = all;

  # ssh user keys
  "root-user-ssh.age".publicKeys = all;
}
