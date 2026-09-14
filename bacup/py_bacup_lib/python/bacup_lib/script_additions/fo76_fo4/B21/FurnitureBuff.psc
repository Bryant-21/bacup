Scriptname B21:FurnitureBuff extends ObjectReference

Struct BuffDatum
    Spell BuffSpell
    Float WaitTime
EndStruct

BuffDatum[] Property Buffs Auto Const

Actor PlayerRef
Bool IsUsing
Int TimerBase

Event OnLoad()
    PlayerRef = Game.GetPlayer()
    RegisterForRemoteEvent(PlayerRef, "OnSit")
    RegisterForRemoteEvent(PlayerRef, "OnGetUp")
EndEvent

Event Actor.OnSit(Actor akSender, ObjectReference akFurniture)
    If akSender != PlayerRef || akFurniture != Self || IsUsing
        Return
    EndIf
    IsUsing = True
    Int thisUse = TimerBase
    Int index = 0
    While index < Buffs.Length && IsUsing && TimerBase == thisUse
        StartTimer(Buffs[index].WaitTime, thisUse + index)
        index += 1
    EndWhile
EndEvent

Event OnTimer(Int aiTimerID)
    Int index = aiTimerID - TimerBase
    If !IsUsing || index < 0 || index >= Buffs.Length
        Return
    EndIf
    If PlayerRef.GetSitState() != 3 || !IsFurnitureInUse(True)
        StopUse()
        Return
    EndIf
    Spell reward = Buffs[index].BuffSpell
    If reward != None
        PlayerRef.DispelSpell(reward)
        If IsUsing && aiTimerID - TimerBase == index
            reward.Cast(PlayerRef, PlayerRef)
        EndIf
    EndIf
EndEvent

Event Actor.OnGetUp(Actor akSender, ObjectReference akFurniture)
    If akSender == PlayerRef && akFurniture == Self
        StopUse()
    EndIf
EndEvent

Event OnUnload()
    StopUse()
    If PlayerRef != None
        UnregisterForRemoteEvent(PlayerRef, "OnSit")
        UnregisterForRemoteEvent(PlayerRef, "OnGetUp")
    EndIf
EndEvent

Event OnReset()
    StopUse()
EndEvent

Function StopUse()
    IsUsing = False
    Int previousUse = TimerBase
    ; Queued callbacks from an earlier use must not reward a later activation.
    TimerBase += Buffs.Length
    Int index = 0
    While index < Buffs.Length
        CancelTimer(previousUse + index)
        index += 1
    EndWhile
EndFunction
