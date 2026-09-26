Quest Function MTNM01_GetOwningQuest()
    Return GetOwningQuest()
EndFunction

Function MTNM01_ReconcileKarmaStage(Form akEquippedItem = None)
    Quest owner = MTNM01_GetOwningQuest()
    If !owner || !owner.IsRunning() || !owner.IsStageDone(ModKarmaStage) || owner.IsStageDone(UseKarmaStage)
        Return
    EndIf
    If akEquippedItem != None && akEquippedItem != MTNM01_PipeSyringer
        Return
    EndIf
    MTNM01QuestScript controller = owner as MTNM01QuestScript
    Actor playerRef = GetActorReference()
    If !controller || !controller.MTNM01_CheckpointValue || !playerRef
        Return
    EndIf
    If playerRef.GetValue(controller.MTNM01_CheckpointValue) >= 2.0
        owner.SetStage(UseKarmaStage)
    EndIf
EndFunction

Event OnPlayerModArmorWeapon(Form akBaseObject, ObjectMod akModBaseObject)
    Quest owner = MTNM01_GetOwningQuest()
    If owner && owner.IsRunning() && owner.IsStageDone(ModKarmaStage) && !owner.IsStageDone(UseKarmaStage) \
        && akBaseObject == MTNM01_PipeSyringer && akModBaseObject == mod_PipeSyringer_Barrel_Karma
        owner.SetStage(UseKarmaStage)
    EndIf
EndEvent

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
