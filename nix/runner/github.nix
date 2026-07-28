{
  config,
  pkgs,
  lib,
  shellInputs,
  ...
}:
let
  workers = 10;
  rootDir = "/var/lib/coln-runner";
  cacheDir = "${rootDir}/cache";
  mkRunner =
    name:
    let
      workDir = "${rootDir}/${name}";
    in
    {
      services.github-runners = {
        ${name} = {
          enable = true;
          name = name;
          replace = true;
          tokenFile = config.age.secrets.runner-token.path;
          tokenType = "access";
          url = "https://github.com/coln-project/Coln";
          extraPackages = with pkgs; [ curl git gcc openssl.dev ] ++ shellInputs;
          extraEnvironment = {
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
            CFLAGS = "-std=gnu17";
            XDG_CACHE_HOME = "${cacheDir}/.cache";
            CARGO_HOME = "${cacheDir}/cargo-home";
            CARGO_TARGET_DIR = "${cacheDir}/cargo-target";
          };
          serviceOverrides = {
            ReadWritePaths = [ rootDir ];
          };
          workDir = "${workDir}";
          user = "coln-runner";
          noDefaultLabels = true;
          extraLabels = [ "coln-runner" ];
        };
      };
      systemd.tmpfiles.rules = [
        "d ${workDir} 0755 coln-runner coln-runner -"
      ];
    };
  runners = lib.mkMerge (
    map (n: mkRunner "coln-runner-${builtins.toString n}") (lib.range 1 workers)
  );
in
{
  config = lib.mkMerge [
    {
      age.secrets.runner-token.file = ./secrets/runner-token.age;
      users.users.coln-runner = {
        isSystemUser = true;
        group = "coln-runner";
      };
      users.groups.coln-runner = { };
      systemd.tmpfiles.rules = [
        "d ${cacheDir} 0755 coln-runner coln-runner -"
      ];
    }
    runners
  ];
}
