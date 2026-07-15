Function ApplyCombatStyle(Actor akActor)
    If akActor == None
        Return
    EndIf

    Int equippedType = akActor.GetEquippedItemType(0)
    If equippedType >= 7 && equippedType <= 10
        If RangedCombatStyle != None
            akActor.SetCombatStyle(RangedCombatStyle)
        EndIf
    ElseIf MeleeCombatStyle != None
        akActor.SetCombatStyle(MeleeCombatStyle)
    EndIf
    akActor.EvaluatePackage()
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
    ApplyCombatStyle(akTarget)
    RegisterForRemoteEvent(akTarget, "OnItemEquipped")
    RegisterForRemoteEvent(akTarget, "OnItemUnequipped")
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    ApplyCombatStyle(akSender)
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    ApplyCombatStyle(akSender)
EndEvent
