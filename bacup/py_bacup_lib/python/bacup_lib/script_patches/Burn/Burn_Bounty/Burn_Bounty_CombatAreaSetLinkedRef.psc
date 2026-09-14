Function CacheCombatAreaTriggers()
    If bTriggersInitialized
        Return
    EndIf
    If HoldUntilEngagedTriggerAlias != None
        refHoldUntilEngagedTrigger = HoldUntilEngagedTriggerAlias.GetReference()
    EndIf
    If HoldPreferredPositionTriggerAlias != None
        refHoldPreferredPositionTrigger = HoldPreferredPositionTriggerAlias.GetReference()
    EndIf
    If HoldPositionTriggerAlias != None
        refHoldPositionTrigger = HoldPositionTriggerAlias.GetReference()
    EndIf
    If SandboxTriggerAlias != None
        refSandboxTrigger = SandboxTriggerAlias.GetReference()
    EndIf
    bTriggersInitialized = refHoldUntilEngagedTrigger != None || refHoldPreferredPositionTrigger != None || refHoldPositionTrigger != None || refSandboxTrigger != None
EndFunction

Function LinkCombatAreaFor(ObjectReference akRef)
    If akRef == None
        Return
    EndIf
    CacheCombatAreaTriggers()
    If refSandboxTrigger != None && SandboxKeyword != None
        akRef.SetLinkedRef(refSandboxTrigger, SandboxKeyword)
    EndIf
    If refHoldUntilEngagedTrigger != None && HoldUntilEngagedKeyword != None
        akRef.SetLinkedRef(refHoldUntilEngagedTrigger, HoldUntilEngagedKeyword)
    EndIf
    ; A melee bounty target must close the distance, so it is held to its own
    ; position instead of a ranged standoff point.
    If BountyIsMelee != None && akRef.HasKeyword(BountyIsMelee)
        If refHoldPositionTrigger != None && HoldPositionKeyword != None
            akRef.SetLinkedRef(refHoldPositionTrigger, HoldPositionKeyword)
        EndIf
    ElseIf refHoldPreferredPositionTrigger != None && HoldPreferredPositionKeyword != None
        akRef.SetLinkedRef(refHoldPreferredPositionTrigger, HoldPreferredPositionKeyword)
    EndIf
EndFunction

Function LinkCombatAreaForCollection()
    Int index = 0
    While index < GetCount()
        LinkCombatAreaFor(GetAt(index))
        index += 1
    EndWhile
EndFunction

Event OnAliasInit()
    bTriggersInitialized = False
    LinkCombatAreaForCollection()
EndEvent

Event OnAliasReset()
    bTriggersInitialized = False
    LinkCombatAreaForCollection()
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    LinkCombatAreaFor(akSenderRef)
EndEvent
