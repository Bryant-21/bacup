Function Fragment_Stage_0010_Item_00()
    ; FO76 initializes the per-player quest instance at this RunOnStart stage.
    ; The FO4 route is initialized by W05_001P_Wayward_QuestScript.OnQuestInit.
    Return
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    If W05_MQ_001P_Wayward_400_Scene
        W05_MQ_001P_Wayward_400_Scene.Start()
    EndIf
    If !IsStageDone(405)
        SetStage(405)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0103_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0105_Item_00()
    SetObjectiveCompleted(100)
    SetStage(200)
EndFunction

Function Fragment_Stage_0301_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerGotQuestFromRoper, 1.0)
    EndIf
    SetStage(300)
EndFunction

Function Fragment_Stage_0302_Item_00()
    SetObjectiveCompleted(200)
    SetStage(300)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(300)
EndFunction

Function Fragment_Stage_0445_Item_00()
    If !IsStageDone(450)
        SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor mortRef = Alias_Mort.GetActorReference()
    If mortRef
        mortRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0455_Item_00()
    Actor batterRef = Alias_Batter.GetActorReference()
    Actor mortRef = Alias_Mort.GetActorReference()
    If batterRef && !batterRef.IsDead()
        batterRef.Kill(mortRef)
    EndIf
EndFunction

Function Fragment_Stage_0460_Item_00()
    Actor batterRef = Alias_Batter.GetActorReference()
    If batterRef
        batterRef.Dismember("Head1", True, False, False)
    EndIf
EndFunction

Function Fragment_Stage_0470_Item_00()
    Actor mortRef = Alias_Mort.GetActorReference()
    If mortRef
        mortRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0491_Item_00()
    If IsStageDone(599) && !IsStageDone(598)
        SetStage(598)
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    ObjectReference playerRef = Alias_owningPlayer.GetReference()
    If playerRef
        Alias_BatterAimTarget.ForceRefTo(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0515_Item_00()
    ObjectReference duchessRef = Alias_Duchess.GetReference()
    If duchessRef
        Alias_BatterAimTarget.ForceRefTo(duchessRef)
    EndIf
EndFunction

Function Fragment_Stage_0516_Item_00()
    ObjectReference duchessRef = Alias_Duchess.GetReference()
    If duchessRef
        Alias_BatterAimTarget.ForceRefTo(duchessRef)
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddToFaction(W05_MQ_001P_BatterEnemyFaction)
    EndIf
EndFunction

Function Fragment_Stage_0522_Item_00()
    Actor batterRef = Alias_Batter.GetActorReference()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If batterRef && playerRef && !batterRef.IsDead()
        playerRef.AddToFaction(W05_MQ_001P_BatterEnemyFaction)
        batterRef.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0530_Item_00()
    Actor batterRef = Alias_Batter.GetActorReference()
    If batterRef
        batterRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerLearnedRadicalsLocation, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0598_Item_00()
    If W05_MQ_001P_Wayward_0599_DuchessInterstatial
        W05_MQ_001P_Wayward_0599_DuchessInterstatial.Start()
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Brew_DuchessDram, 1, False)
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerGotFreeDrink, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerAskedAboutOverseer, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0660_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_002, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0680_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerAgreedToHearOutDuchess, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0705_Item_00()
    ; FO76 grants the Hunter for Hire reward through GMRW 6313B0, not a VMAD property.
    ; The single-player quest route does not depend on that online reward service.
    Return
EndFunction

Function Fragment_Stage_0710_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_003, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0809_Item_00()
    ObjectReference schematicMarker = Alias_SchematicEnableMarker.GetReference()
    If schematicMarker
        schematicMarker.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    ObjectReference gorgeMapMarker = Alias_GorgeMapMarker.GetReference()
    If gorgeMapMarker
        gorgeMapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If W05_MQ_001P_Wayward_500_Scene
        W05_MQ_001P_Wayward_500_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0599_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_BatterDied, 1.0)
    EndIf
    If IsStageDone(500) && !IsStageDone(598)
        SetStage(598)
    EndIf
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0805_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0807_Item_00()
    If !IsStageDone(809)
        SetStage(809)
    EndIf
    Actor duchessRef = Alias_Duchess.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
    AttemptRadicalHandoff()
EndFunction

Function Fragment_Stage_0905_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
    AttemptRadicalHandoff()
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(600)
    CompleteQuest()
EndFunction

Function Fragment_Stage_1000_Item_00()
    If W05_MQ_003P_Muscle_QuestStartKeyword
        W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()
    EndIf
EndFunction

Function AttemptRadicalHandoff()
    ObjectReference playerRef = Alias_owningPlayer.GetReference()
    Quest radicalQuest = Game.GetFormFromFile(0x0040F5BE, "SeventySix.esm") as Quest
    Bool accepted = False
    If playerRef && radicalQuest && W05_MQ_002P_Radical_QuestStartKeyword
        accepted = radicalQuest.IsRunning() || radicalQuest.IsCompleted()
        If !accepted
            W05_MQ_002P_Radical_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
            accepted = radicalQuest.IsRunning() || radicalQuest.IsCompleted()
        EndIf
    EndIf
    If !accepted && IsStageDone(9000)
        StartTimer(5.0, 9000)
    ElseIf accepted && IsStageDone(9000)
        ; Mort's activation barks remain eligible until this higher-priority quest stops.
        CancelTimer(9000)
        Stop()
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9000 && IsStageDone(9000)
        AttemptRadicalHandoff()
    EndIf
EndEvent
