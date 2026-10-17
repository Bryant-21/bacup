; Each living Watoga robot carrying the ability adds one to the robot-population flip sign; death or losing the
; ability takes it off again. The effect's sign property ships unbound, so the placed Watoga sign 4F82B5 is the fallback.
Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget != None && !akTarget.IsDead()
        AdjustRobotCount(1)
    EndIf
EndEvent

Event OnDeath(Actor akKiller)
    AdjustRobotCount(-1)
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget != None && !akTarget.IsDead()
        AdjustRobotCount(-1)
    EndIf
EndEvent

Function AdjustRobotCount(Int aiDelta)
    ObjectReference signRef = FlipCardSignManager_WatogaRobotPop
    If signRef == None
        signRef = Game.GetFormFromFile(0x004F82B5, "SeventySix.esm") as ObjectReference
    EndIf
    FlipCardSignCounterScript sign = signRef as FlipCardSignCounterScript
    If sign == None
        Return
    EndIf
    sign.Count = Math.Max(0, sign.Count + aiDelta) as Int
    If sign.Is3DLoaded()
        sign.DisplayCount()
    EndIf
EndFunction
