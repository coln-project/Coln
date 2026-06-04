module Geolog.Core.Value where

import Data.Vector.Strict qualified as Vector

import Geolog.Common
import Geolog.Core.Params

-- Locals
--------------------------------------------------------------------------------

data Locals = LNil | LSnoc Locals (El N) | LSnocChunk Locals (Vector (El N))

instance ElemAt Locals BId (El N) where
  elemAt LNil _ = panic "no such index"
  elemAt (LSnoc e v) i
    | i == 0 = v
    | otherwise = elemAt e (i - 1)
  elemAt (LSnocChunk e vs) (BId i)
    | i < Vector.length vs = vs Vector.! i
    | otherwise = elemAt e (BId (i - Vector.length vs))

-- Evaluations
--------------------------------------------------------------------------------

type family Evaluation (f :: Case -> Type) (c :: Case) = r | r -> c f where
  Evaluation f N = f N
  Evaluation f D = Description f

data Description :: (Case -> Type) -> Type where
  Describe :: f D -> Description f
  Become :: f N -> Description f

class HasEvaluation (c :: Case) where
  epure :: a c -> Evaluation a c
  emap :: (forall c'. HasEvaluation c' => a c' -> b c') -> Evaluation a c -> Evaluation b c
  ebind :: (forall c'. HasEvaluation c' => a c' -> Evaluation b c') -> Evaluation a c -> Evaluation b c
  scase :: SCase c

instance HasEvaluation N where
  epure = id
  emap f = f
  ebind f = f
  scase = SNominative

instance HasEvaluation D where
  epure = Describe
  emap f (Describe x) = Describe (f x)
  emap f (Become x) = Become (f x)
  ebind f (Describe x) = f x
  ebind f (Become x) = Become (f x)
  scase = SDescriptive

-- Abstractions
--------------------------------------------------------------------------------

data Clo (f :: Case -> Type) (c :: Case)
  = Clo Name Locals (Locals -> (Evaluation f c))
  | CloConst (Evaluation f c)

appClo :: Clo f c -> El N -> Evaluation f c
appClo (Clo _ locals body) v = body (LSnoc locals v)
appClo (CloConst body) _ = body

-- Neutrals
--------------------------------------------------------------------------------

data Spine
  = Id
  | App Spine (El N)
  | Proj Spine Name

data Head
  = LocalVar FId
  | GlobalVar Name ~(El N)

data Expansion
  = IntoCons (Dict (El N))
  | NotApplicable

data Neutral = Neutral
  { head :: Head
  , spine :: Spine
  , ty :: ~(Ty N)
  , expansion :: ~Expansion
  , description :: ~(Maybe (El D))
  }

unwrap :: Expansion -> Dict (El N)
unwrap (IntoCons d) = d
unwrap NotApplicable = panic "neutral of record type was never expanded"

expandRecord :: RecordType -> Head -> Spine -> Maybe (El D) -> Dict (El N)
expandRecord recordType head spine desc = do
  let go :: Locals -> [(Name, Locals -> Ty N)] -> [El N]
      go _ [] = []
      go vs ((x, ty):rest) = do
        let v = reflect head (Proj spine x) (ty vs) ((`proj` x) <$> desc)
        v : go (LSnoc vs v) rest
  let tele = recordType.fieldTypes
  Dict
    tele.head
    (Vector.fromList (go recordType.capture (toList tele)))

reflect :: Head -> Spine -> Ty N -> Maybe (Evaluation El D) -> El N
reflect head spine ~ty edesc = do
  let k desc = do
        let ~expansion = case behavior ty of
              LikeRecord recordType ->
                IntoCons (expandRecord recordType head spine desc)
              _ -> NotApplicable
        Neu $ Neutral head spine ty expansion desc
  case edesc of
    Just (Describe desc) -> k (Just desc)
    Just (Become v) -> v
    Nothing -> k Nothing

local :: FId -> Ty N -> El N
local i a = reflect (LocalVar i) Id a Nothing

data DecodedNeutral = DecodedNeutral
  { head :: Head
  , spine :: Spine
  , universe :: Universe
  , behavior :: ~TypeBehavior
  }

data BareNeutral = BareNeutral
  { head :: Head
  , spine :: Spine
  }

class ToBare n where
  toBare :: n -> BareNeutral

instance ToBare Neutral where
  toBare n = BareNeutral n.head n.spine

instance ToBare DecodedNeutral where
  toBare n = BareNeutral n.head n.spine

-- Elements
--------------------------------------------------------------------------------

data El :: Case -> Type where
  Neu :: Neutral -> El N
  Code :: Ty c -> El c
  Lam :: ~(Ty N) -> Clo El c -> El c
  Cons :: Dict (Evaluation El c) -> El c
  Lit :: Literal -> El N

app :: El c -> El N -> Evaluation El c
app (Lam _ clo) arg = appClo clo arg
app (Neu n) arg =
  reflect n.head (App n.spine arg) (appTy n.ty arg) ((`app` arg) <$> n.description)
app _ _ = panic "ill-typed application"

coerceToFields :: El c -> Dict (Evaluation El c)
coerceToFields (Cons fields) = fields
coerceToFields (Neu n) = case n.expansion of
  IntoCons fields -> fields
  _ -> panic "unexpanded neutral of record type"
coerceToFields _ = panic "ill-typed projection"

proj :: El c -> Name -> Evaluation El c
proj v x = elemAt (coerceToFields v) x

-- Types
--------------------------------------------------------------------------------

data FunctionType = FunctionType
  { variant :: FunctionVariant
  , dom :: Ty N
  , cod :: Clo Ty N
  }

data RecordType = RecordType
  { level :: Level
  , capture :: Locals
  , fieldTypes :: Dict (Locals -> Ty N)
  }

data EqualityType = EqualityType
  { at :: Ty N
  , lhs :: El N
  , rhs :: El N
  }

typeForProjection :: RecordType -> Name -> Dict (El N) -> Ty N
typeForProjection rt x fields = do
  let i = getKeyIndex rt.fieldTypes x
  let chunk = Vector.slice 0 i.value fields.values
  let locals = LSnocChunk rt.capture chunk
  elemAt rt.fieldTypes i $ locals

data Ty :: Case -> Type where
  U :: Universe -> Ty N
  Decode :: DecodedNeutral -> Ty N
  Function :: FunctionType -> Ty N
  Record :: RecordType -> Ty D
  Eq :: EqualityType -> Ty N
  BuiltinTy :: BuiltinTy -> Ty N

instance LevelOf (Ty c) where
  levelOf = \case
    U u -> codesInto u
    Decode n -> decodesInto n.universe
    Function ft -> levelOf ft.variant
    Record rt -> rt.level
    Eq ety -> levelOf ety.at
    BuiltinTy _ -> Set

behavior :: Ty c -> TypeBehavior
behavior = \case
  U u -> LikeU u
  Decode n -> n.behavior
  Function ft -> LikeFunction ft
  Record rt -> LikeRecord rt
  Eq _ -> NoRules
  BuiltinTy bty -> LikeBuiltinTy bty

decode :: (HasEvaluation c) => El c -> Evaluation Ty c
decode (Code a) = epure a
decode (Neu n) = do
  let u = case n.ty of
        U u' -> u'
        _ -> panic "ill-typed decoding"
  let k desc = Decode (DecodedNeutral n.head n.spine u desc)
  case decode <$> n.description of
    Just (Describe desc) -> k (behavior desc)
    Just (Become ty) -> ty
    Nothing -> k NoRules
    
decode _ = panic "ill-typed decoding"

-- Type behavior
--------------------------------------------------------------------------------

data TypeBehavior
  = LikeU Universe
  | LikeFunction FunctionType
  | LikeRecord RecordType
  | LikeBuiltinTy BuiltinTy
  | NoRules

appTy :: Ty N -> El N -> Ty N
appTy (behavior -> LikeFunction ft) arg = appClo ft.cod arg
appTy _ _ = panic "ill-typed application"

projTy :: Ty N -> El N -> Name -> Ty N
projTy (behavior -> LikeRecord rt) v x =
  typeForProjection rt x (coerceToFields v)
projTy _ _ _ = panic "ill-typed projection"
