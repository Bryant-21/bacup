Function Fragment_Stage_0001_Item_00()
    GiveAliasItem(Alias_MissionLog)
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Alias_BypassTape.GetRef() != None
        playerRef.MoveTo(Alias_BypassTape.GetRef())
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        Alias_currentPlayer.ForceRefTo(playerRef)
        Alias_BypassTapeName.ForceRefTo(Alias_BypassTape.GetRef())
        playerRef.SetValue(EN01_Bunker_StartedValue, 1.0)
    EndIf
    If EN01_MQ_Bunker_Master != None && !EN01_MQ_Bunker_Master.IsRunning()
        EN01_MQ_Bunker_Master.Start()
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    Alias_AgentCorpse.TryToEnableNoWait()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    GiveAliasItem(Alias_MissionLog)
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0015_Item_00()
    If IsStageDone(20)
        SetStage(30)
    Else
        SetObjectiveDisplayed(15, True)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    HandleMissionLogListened()
EndFunction

Function Fragment_Stage_0020_Item_01()
    HandleMissionLogListened()
EndFunction

Function Fragment_Stage_0025_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        Alias_PlayerPoppedObj.ForceRefTo(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveCompleted(15, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0040_Item_00()
    Alias_currentPlayer.TryToSetValue(EN01_PlayerTriggeredBypassValue, 1.0)
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(50, True)
    EN01_Sam_KeypadBypassScene.Start()
EndFunction

Function Fragment_Stage_0050_Item_00()
    Alias_currentPlayer.TryToSetValue(EN01_CheckpointValue, 1.0)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
    If !IsStageDone(61)
        SetStage(61)
    EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(70, True)
    SetObjectiveDisplayed(83, True)
    SetObjectiveDisplayed(86, True)
    SetObjectiveDisplayed(89, True)
EndFunction

Function Fragment_Stage_0080_Item_00()
    SetObjectiveDisplayed(70, True)
    SetObjectiveDisplayed(83, True)
    SetObjectiveDisplayed(86, True)
    SetObjectiveDisplayed(89, True)
EndFunction

Function Fragment_Stage_0083_Item_00()
    SetObjectiveCompleted(83, True)
    EN01_MQ_Bunker_BreakerActivated.Start()
    CheckComponentReset()
EndFunction

Function Fragment_Stage_0086_Item_00()
    SetObjectiveCompleted(86, True)
    EN01_MQ_Bunker_FlueActivated.Start()
    CheckComponentReset()
EndFunction

Function Fragment_Stage_0089_Item_00()
    SetObjectiveCompleted(89, True)
    EN01_MQ_Bunker_ConduitActivated.Start()
    CheckComponentReset()
EndFunction

Function Fragment_Stage_0090_Item_00()
    Alias_currentPlayer.TryToSetValue(EN01_PlayerTriggeredReset, 1.0)
    EN01_BunkerQuestScript masterScript = EN01_MQ_Bunker_Master as EN01_BunkerQuestScript
    If masterScript != None
        masterScript.BeginReset()
    EndIf
    EN01_Sam_Reset_PowerSystem.Start()
EndFunction

Function Fragment_Stage_0095_Item_00()
    Alias_currentPlayer.TryToSetValue(EN01_PlayerTriggeredReset, 1.0)
    EN01_BunkerQuestScript masterScript = EN01_MQ_Bunker_Master as EN01_BunkerQuestScript
    If masterScript != None
        masterScript.BeginReset()
    EndIf
    EN01_Sam_Reset_CredentialTerminalCredential.Start()
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveCompleted(83, True)
    SetObjectiveCompleted(86, True)
    SetObjectiveCompleted(89, True)
    SetObjectiveDisplayed(100, True)
    Int index = 0
    While index < Alias_LaserGrids.GetCount()
        ObjectReference gridRef = Alias_LaserGrids.GetAt(index)
        If gridRef != None
            gridRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile
    EN01_BunkerQuestScript masterScript = EN01_MQ_Bunker_Master as EN01_BunkerQuestScript
    If masterScript != None
        masterScript.FinishReset()
    EndIf
EndFunction

Function Fragment_Stage_0102_Item_00()
    If IsStageDone(90) || IsStageDone(95)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    Alias_currentPlayer.TryToSetValue(EN01_PlayerRegisteredHandprint, 1.0)
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(110, True)
    EN01_Sam_HandscannerScene.Start()
EndFunction

Function Fragment_Stage_0112_Item_00()
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(120, True)
    SetObjectiveDisplayed(122, True)
EndFunction

Function Fragment_Stage_0114_Item_00()
    Alias_DivorceNote.TryToEnableNoWait()
EndFunction

Function Fragment_Stage_0115_Item_00()
    SetObjectiveDisplayed(124, True)
EndFunction

Function Fragment_Stage_0118_Item_00()
    Alias_IntelNote.TryToEnableNoWait()
EndFunction

Function Fragment_Stage_0119_Item_00()
    SetObjectiveDisplayed(128, True)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(120, True)
    Alias_DiaryContainer.TryToEnableNoWait()
    Alias_DivorceNoteContainer.TryToEnableNoWait()
    Alias_IntelNoteContainer.TryToEnableNoWait()
    Alias_WelcomeNoteContainer.TryToEnableNoWait()
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(120, True)
    SetObjectiveDisplayed(150, True)
    Alias_SafePainting.TryToEnableNoWait()
    Alias_PlayerCanAccessPainting.ForceRefTo(Game.GetPlayer())
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(150, True)
    SetObjectiveDisplayed(200, True)
    ObjectReference paintingRef = Alias_SafePainting.GetRef()
    If paintingRef != None
        QSTEN01PaintingMove.Play(paintingRef)
    EndIf
    GiveAliasItem(Alias_BlackwellID)
    GiveAliasItem(Alias_WhitespringHolotape)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.AddItem(EN01_SafePassword, 1, True)
        playerRef.SetValue(EN01_PlayerKnowsSamBlackwell, 1.0)
        ITMHolotapeUp.Play(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    FinishBunkerQuest(True)
EndFunction

Function Fragment_Stage_0260_Item_00()
    FinishBunkerQuest(False)
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteAllObjectives()
    CompleteQuest()
    If EN01_MQ_Bunker_Master != None && EN01_MQ_Bunker_Master.IsRunning()
        EN01_MQ_Bunker_Master.Stop()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    FailAllObjectives()
    If EN01_MQ_Bunker_Master != None && EN01_MQ_Bunker_Master.IsRunning()
        EN01_MQ_Bunker_Master.Stop()
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_0950_Item_00()
    RemoveAliasItem(Alias_MissionLog)
    RemoveAliasItem(Alias_BypassTape)
    RemoveAliasItem(Alias_DiaryPage)
    RemoveAliasItem(Alias_DivorceNote)
    RemoveAliasItem(Alias_IntelNote)
    RemoveAliasItem(Alias_WelcomeSenatorNote)
EndFunction

Function HandleMissionLogListened()
    SetObjectiveCompleted(10, True)
    If IsStageDone(15)
        SetStage(30)
    Else
        SetObjectiveDisplayed(15, True)
    EndIf
EndFunction

Function CheckComponentReset()
    If IsStageDone(83) && IsStageDone(86) && IsStageDone(89) && !IsStageDone(90)
        SetStage(90)
    EndIf
EndFunction

Function FinishBunkerQuest(Bool abStartEN02)
    SetObjectiveCompleted(200, True)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && EN01_CompletedValue != None && playerRef.GetValue(EN01_CompletedValue) < 1.0
        playerRef.SetValue(EN01_CompletedValue, 1.0)
    EndIf
    If abStartEN02
        StartEN02MainQuest()
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function GiveAliasItem(ReferenceAlias itemAlias)
    Actor playerRef = Game.GetPlayer()
    ObjectReference itemRef = itemAlias.GetRef()
    If playerRef != None && itemRef != None && playerRef.GetItemCount(itemRef.GetBaseObject()) <= 0
        playerRef.AddItem(itemRef, 1, False)
    EndIf
EndFunction

Function RemoveAliasItem(ReferenceAlias itemAlias)
    Actor playerRef = Game.GetPlayer()
    ObjectReference itemRef = itemAlias.GetRef()
    If playerRef != None && itemRef != None
        playerRef.RemoveItem(itemRef.GetBaseObject(), playerRef.GetItemCount(itemRef.GetBaseObject()), True)
    EndIf
EndFunction

Function StartEN02MainQuest()
    Actor playerRef = Game.GetPlayer()
    If EN02_QuestStartKeyword != None && playerRef != None
        EN02_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
