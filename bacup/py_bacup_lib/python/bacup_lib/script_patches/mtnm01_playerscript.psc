Quest Function MTNM01_GetOwningQuest()
    Return GetOwningQuest()
EndFunction

Function MTNM01_ReconcileKarmaStage(Form akEquippedItem = None)
    Quest owner = MTNM01_GetOwningQuest()
    If !owner || !owner.IsRunning() || !owner.IsStageDone(250) || owner.IsStageDone(UseKarmaStage)
        Return
    EndIf
    If akEquippedItem == None || akEquippedItem == MTNM01_PipeSyringer
        owner.SetStage(UseKarmaStage)
    EndIf
EndFunction

Function StartSuperMutantStage()
    Quest owner = MTNM01_GetOwningQuest()
    If owner && owner.IsRunning() && owner.IsStageDone(700) && !owner.IsStageDone(750)
        owner.SetStage(750)
    EndIf
EndFunction

Event OnAliasInit()
    MTNM01_ReconcileKarmaStage()
EndEvent

Event OnPlayerLoadGame()
    MTNM01_ReconcileKarmaStage()
EndEvent

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    MTNM01_ReconcileKarmaStage(akBaseObject)
EndEvent
