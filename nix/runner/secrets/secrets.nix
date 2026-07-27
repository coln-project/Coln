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
  "runner-token.age".publicKeys = all;
  "wifi.age".publicKeys = all;
}
