-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.Strings where

import Data.String (IsString (..))
import Data.Text (Text)
import Data.Text qualified as T
import Diagnostician
import FNotation (Name (..))
import Prettyprinter
import Prettyprinter.Render.String
import Prettyprinter.Render.Text

mangleSeg :: Text -> DDoc
mangleSeg = pretty . T.map (\c -> if c == '-' then '_' else c)

mangleToDoc :: Name -> DDoc
mangleToDoc x = mconcat [mangleSeg s <> "_slash_" | s <- x.init] <> mangleSeg x.last

mangleToString :: Name -> String
mangleToString = renderString . layoutPretty defaultLayoutOptions . mangleToDoc

fromShow :: (Show a, IsString b) => a -> b
fromShow = fromString . show

renderText :: DDoc -> Text
renderText = renderStrict . layoutCompact
