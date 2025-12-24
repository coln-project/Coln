module Geolog.Common where

import Control.Monad.IO.Class
import Control.Monad.ST (RealWorld)
import Control.Monad.State.Class
import Data.Hashable
import Data.String (IsString, fromString)
import Data.Text (Text)
import Data.Vector.Generic qualified as VG
import Data.Vector.Generic.Mutable qualified as VGM
import Data.Vector.Hashtables (FrozenDictionary)
import Data.Vector.Hashtables qualified as HT
import Data.Vector.Strict qualified as V
import Lens.Micro.Platform
import Prettyprinter
import Symbolize (Symbol, unintern)
import System.IO.Unsafe (unsafePerformIO)

newtype Name = Name Symbol
  deriving (Eq, Hashable) via Symbol

instance Show Name where
  show (Name s) = unintern s

instance IsString Name where
  fromString s = Name (fromString s)

instance Pretty Name where
  pretty (Name s) = pretty (unintern s :: Text)

type Pos = Int

data Span = Span
  { spanStart :: Pos
  , spanEnd :: Pos
  }
  deriving (Eq, Show)

instance Pretty Span where
  pretty (Span s e) = pretty s <> ":" <> pretty e

type Bwd a = [a]

infixl 5 :>

pattern (:>) :: Bwd a -> a -> Bwd a
pattern xs :> x = x : xs

type Fwd a = [a]

infixr 5 :<

pattern (:<) :: a -> Fwd a -> Fwd a
pattern x :< xs = x : xs

newtype ConfTable v = ConfTable (FrozenDictionary V.Vector Name V.Vector v)

class ElemAt a i b | a -> i b where
  elemAt :: a -> i -> b

class Lookup a i b | a -> i b where
  lookup :: a -> i -> Maybe b

class Contains a i | a -> i where
  contains :: a -> i -> Bool

instance Lookup (ConfTable v) Name v where
  lookup (ConfTable d) x = case HT.findElem d x of
    -1 -> Nothing
    i -> Just (HT.fvalue d V.! i)

class FromList a e | a -> e where
  fromList :: [e] -> a

instance FromList (ConfTable v) (Name, v) where
  fromList l = unsafePerformIO do
    d <- HT.fromList l
    fd <- HT.unsafeFreeze d
    pure $ ConfTable fd

-- Our custom annotations for docs
data Ann = AText

data Buffer v e = Buffer Int (v RealWorld e)

push ::
  (MonadState s m, MonadIO m, VGM.MVector v e) =>
  Lens' s (Buffer v e) ->
  e ->
  m ()
push bl x = do
  Buffer l v <- use bl
  let cap = VGM.length v
  v' <-
    if cap <= l
      then liftIO $ VGM.unsafeGrow v cap
      else pure v
  liftIO $ VGM.unsafeWrite v' l x
  bl .= Buffer (l + 1) v'

bufferWithCapacity :: (VGM.MVector v e) => Int -> IO (Buffer v e)
bufferWithCapacity c = Buffer 0 <$> VGM.unsafeNew c

bufferUnsafeFreeze :: (VG.Vector v e) => Buffer (VG.Mutable v) e -> IO (v e)
bufferUnsafeFreeze (Buffer l v) = VG.take l <$> VG.unsafeFreeze v
