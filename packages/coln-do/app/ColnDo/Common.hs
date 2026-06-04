module ColnDo.Common (
  module Development.Shake,
  module Development.Shake.FilePath,
  getProjects,
  projectPaths,
  getHsFiles,
)
where

import Development.Shake
import Development.Shake.FilePath

getProjects :: Action [String]
getProjects = do
  cabalFiles <- getDirectoryFiles "" ["packages/*/*.cabal"]
  pure $ takeDirectory <$> cabalFiles

projectPaths :: [String]
projectPaths = ["src", "test", "app", "src-bin"]

getHsFiles :: Action [String]
getHsFiles = do
  hsProjects <- getProjects
  let paths = [proj ++ "/" ++ path ++ "//*.hs" | proj <- hsProjects, path <- projectPaths]
  getDirectoryFiles "" paths
