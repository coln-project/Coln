{agenix, ...}:{
  imports = [agenix.nixosModules.default];

  config = {
    age.secrets = {
      wifi = {
        file = ./secrets/wifi.age;
        mode = "0440";
        group = "wpa_supplicant";
      };
      runner-token.file = ./secrets/runner-token.age;
    };
  };
}
