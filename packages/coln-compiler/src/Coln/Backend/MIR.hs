-- | MIR stands for "model intermediate representation"
--   it is for expressing *models* of theories, possibly in the context of free
--   variables which are set-level.
module Coln.Backend.MIR where

-- If layout is Core -> MIR, then we need MIR values as well as MIR syntax.

-- Specifically, this is because layout will have to be MIR value -> MIR syntax.

data El
  = Var BId
  | 
