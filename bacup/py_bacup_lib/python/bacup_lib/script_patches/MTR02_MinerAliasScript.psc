Event OnAliasInit()
    Actor player = GetReference() as Actor
    If player != None
        RegisterForRemoteEvent(player, "OnItemEquipped")
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
        ReconcileEquippedArmor()
    EndIf
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    Quest minerQuest = GetOwningQuest()
    If akSender != Game.GetPlayer() || akBaseObject == None || minerQuest == None || !minerQuest.IsRunning() || minerQuest.GetStage() < 30 || minerQuest.IsStageDone(255)
        Return
    EndIf

    MTR02_MinerQuestScript minerQuestScript = minerQuest as MTR02_MinerQuestScript
    If minerQuestScript != None
        minerQuestScript.GrantExcavatorArmorForLocalProgress()
    EndIf

    If akBaseObject.HasKeyword(MTR02_MinerBuiltLeftArm) && !minerQuest.IsStageDone(40)
        minerQuest.SetStage(40)
    ElseIf akBaseObject.HasKeyword(MTR02_MinerBuiltRightArm) && !minerQuest.IsStageDone(50)
        minerQuest.SetStage(50)
    ElseIf akBaseObject.HasKeyword(MTR02_MinerBuiltHelmet) && !minerQuest.IsStageDone(60)
        minerQuest.SetStage(60)
    ElseIf akBaseObject.HasKeyword(MTR02_MinerBuiltTorso) && !minerQuest.IsStageDone(70)
        minerQuest.SetStage(70)
    ElseIf akBaseObject.HasKeyword(MTR02_MinerBuiltLeftLeg) && !minerQuest.IsStageDone(80)
        minerQuest.SetStage(80)
    ElseIf akBaseObject.HasKeyword(MTR02_MinerBuiltRightLeg) && !minerQuest.IsStageDone(90)
        minerQuest.SetStage(90)
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterForRemoteEvent(akSender, "OnItemEquipped")
        MTR02_MinerQuestScript minerQuestScript = GetOwningQuest() as MTR02_MinerQuestScript
        If minerQuestScript != None
            minerQuestScript.GrantExcavatorArmorForLocalProgress()
        EndIf
        ReconcileEquippedArmor()
    EndIf
EndEvent

Event OnAliasShutdown()
    Actor player = GetReference() as Actor
    If player != None
        UnregisterForRemoteEvent(player, "OnItemEquipped")
        UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
EndEvent

Function ReconcileEquippedArmor()
    Actor player = GetReference() as Actor
    Quest minerQuest = GetOwningQuest()
    If player == None || minerQuest == None || !minerQuest.IsRunning() || minerQuest.GetStage() < 30 || minerQuest.IsStageDone(255)
        Return
    EndIf

    If player.WornHasKeyword(MTR02_MinerBuiltLeftArm) && !minerQuest.IsStageDone(40)
        minerQuest.SetStage(40)
    EndIf
    If player.WornHasKeyword(MTR02_MinerBuiltRightArm) && !minerQuest.IsStageDone(50)
        minerQuest.SetStage(50)
    EndIf
    If player.WornHasKeyword(MTR02_MinerBuiltHelmet) && !minerQuest.IsStageDone(60)
        minerQuest.SetStage(60)
    EndIf
    If player.WornHasKeyword(MTR02_MinerBuiltTorso) && !minerQuest.IsStageDone(70)
        minerQuest.SetStage(70)
    EndIf
    If player.WornHasKeyword(MTR02_MinerBuiltLeftLeg) && !minerQuest.IsStageDone(80)
        minerQuest.SetStage(80)
    EndIf
    If player.WornHasKeyword(MTR02_MinerBuiltRightLeg) && !minerQuest.IsStageDone(90)
        minerQuest.SetStage(90)
    EndIf
EndFunction
