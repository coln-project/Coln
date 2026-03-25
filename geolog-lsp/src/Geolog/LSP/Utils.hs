module Geolog.LSP.Utils where

import Control.Lens
import Data.Maybe
import Data.Text qualified as T
import Language.LSP.Protocol.Lens
import Language.LSP.Protocol.Types
import Language.LSP.Server
import Language.LSP.VFS

currentBufferText :: forall config m s a1 a2. (MonadLsp config m, HasParams s a1, HasTextDocument a1 a2, HasUri a2 Uri) => s -> m T.Text
currentBufferText = fmap (virtualFileText . fromJust) . (getVirtualFile . currentBufferUri)

currentBufferUri :: (HasParams s a1, HasTextDocument a1 a2, HasUri a2 Uri) => s -> NormalizedUri
currentBufferUri = toNormalizedUri . view currentBufferUriUnNormalized

currentBufferUriUnNormalized :: (HasParams s a1, HasTextDocument a1 a2, HasUri a2 a3) => Lens' s a3
currentBufferUriUnNormalized = params . textDocument . uri

