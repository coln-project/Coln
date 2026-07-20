{agenix, ...}:{
  imports = [agenix.nixosModules.default];

  config = {
    age.secrets = {
      wifi = {
        file = ./secrets/wifi.age;
        mode = "0440";
        group = "wpa_supplicant";
      };
      root-pass.file = ./secrets/root-pass.age;
      runner-token.file = ./secrets/runner-token.age;
      root-user-ssh = {
        file = ./secrets/root-user-ssh.age;
        path = "/root/.ssh/id_ed25519";
        owner = "root";
        mode = "0600";
      };
    };
  };
}
