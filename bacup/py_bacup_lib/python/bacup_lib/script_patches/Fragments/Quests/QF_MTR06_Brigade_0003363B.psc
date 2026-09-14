Actor Function MTR06_GetPlayer()
    Actor playerRef = Alias_ActivePlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
        If playerRef != None && Alias_ActivePlayer.GetReference() == None
            Alias_ActivePlayer.ForceRefTo(playerRef)
        EndIf
    EndIf
    Return playerRef
EndFunction

Function MTR06_RecordStage(Int stageID)
    Actor playerRef = MTR06_GetPlayer()
    If playerRef != None
        playerRef.SetValue(MTR06_QuestStage, stageID as Float)
    EndIf
EndFunction

Function MTR06_GiveAliasItem(Actor playerRef, ReferenceAlias itemAlias)
    If playerRef == None || itemAlias == None
        Return
    EndIf

    ObjectReference itemRef = itemAlias.GetReference()
    If itemRef != None && itemRef.GetBaseObject() != None && playerRef.GetItemCount(itemRef.GetBaseObject()) < 1
        playerRef.AddItem(itemRef.GetBaseObject(), 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    If playerRef != None
        playerRef.SetValue(MTR06_TriggeredMiscValue, 1.0)
    EndIf
    SetObjectiveDisplayed(3)
    MTR06_RecordStage(3)
EndFunction

Function Fragment_Stage_0004_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    If playerRef != None
        playerRef.SetValue(MTR06_QuestStarted, 1.0)
    EndIf
    MTR06_RecordStage(4)
EndFunction

Function Fragment_Stage_0005_Item_00()
    SetObjectiveCompleted(3)
    SetObjectiveDisplayed(5)
    MTR06_RecordStage(5)
EndFunction

Function Fragment_Stage_0010_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(15)
    If playerRef != None
        Alias_PlayerReadyForKnowledgeExam.ForceRefTo(playerRef)
        playerRef.SetValue(MTR06_CheckpointValue, 1.0)
    EndIf
    MTR06_RecordStage(10)
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveCompleted(15)
    MTR06_RecordStage(15)
EndFunction

Function Fragment_Stage_0020_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(15, False)
    SetObjectiveDisplayed(20)
    Alias_PlayerReadyForKnowledgeExam.Clear()
    If playerRef != None
        Alias_PlayerReadyForPhysExam.ForceRefTo(playerRef)
        playerRef.SetValue(MTR06_CheckpointValue, 2.0)
    EndIf
    SetStage(22)
    MTR06_RecordStage(20)
EndFunction

Function Fragment_Stage_0029_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    If playerRef != None
        playerRef.SetValue(MTR06_PhysExamCompleted, 1.0)
    EndIf
    MTR06_RecordStage(29)
    If !IsStageDone(31)
        SetStage(31)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    If playerRef != None
        playerRef.SetValue(MTR06_PhysExamCompleted, 1.0)
    EndIf
    MTR06_RecordStage(30)
    If !IsStageDone(31)
        SetStage(31)
    EndIf
EndFunction

Function Fragment_Stage_0031_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    Alias_PlayerReadyForPhysExam.Clear()
    If playerRef != None
        Alias_PlayerReadyForFinalExam.ForceRefTo(playerRef)
        playerRef.SetValue(MTR06_CheckpointValue, 3.0)
    EndIf
    If CheckpointMessage != None
        CheckpointMessage.Show()
    EndIf
    If !IsStageDone(32)
        SetStage(32)
    EndIf
    MTR06_RecordStage(31)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
    Alias_PlayerReadyForFinalExam.Clear()
    If MTR06_Brigade_0035_FinalExamIntro != None && !MTR06_Brigade_0035_FinalExamIntro.IsPlaying()
        MTR06_Brigade_0035_FinalExamIntro.Start()
    EndIf
    MTR06_RecordStage(40)
EndFunction

Function Fragment_Stage_0045_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
    MTR06_GiveAliasItem(playerRef, Alias_UniformTicket)
    MTR06_RecordStage(45)
    If !IsStageDone(46)
        SetStage(46)
    EndIf
EndFunction

Function Fragment_Stage_0046_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    If playerRef != None
        Alias_PlayerCanCollectUniform.ForceRefTo(playerRef)
    EndIf
    MTR06_RecordStage(46)
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(50)
    Alias_PlayerCanCollectUniform.Clear()
    If playerRef != None
        ObjectReference ticketRef = Alias_UniformTicket.GetReference()
        If ticketRef != None && ticketRef.GetBaseObject() != None
            playerRef.RemoveItem(ticketRef.GetBaseObject(), 1, True)
        EndIf
        If ClothesFiremanUniform != None && playerRef.GetItemCount(ClothesFiremanUniform) < 1
            playerRef.AddItem(ClothesFiremanUniform, 1, True)
        EndIf
        If ClothesFiremanHat != None && playerRef.GetItemCount(ClothesFiremanHat) < 1
            playerRef.AddItem(ClothesFiremanHat, 1, True)
        EndIf
        MTR06_GiveAliasItem(playerRef, Alias_AntiScorched10mm)
        MTR06_GiveAliasItem(playerRef, Alias_FinalExamHolotape)
        playerRef.SetValue(MTR06_GaveOutMidQuestReward, 1.0)
    EndIf
    MTR06_RecordStage(50)
EndFunction

Function Fragment_Stage_0055_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(55)
    If playerRef != None
        Alias_PlayerCanTriggerBeacon.ForceRefTo(playerRef)
    EndIf
    If MTR06_TrainingMine != None && !MTR06_TrainingMine.IsRunning()
        MTR06_TrainingMine.Start()
    EndIf
    MTR06_RecordStage(55)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(60)
    Alias_PlayerCanTriggerBeacon.Clear()
    MTR06_RecordStage(60)
EndFunction

Function Fragment_Stage_0065_Item_00()
    Alias_FinalExamRespawnTarget.Clear()
    MTR06_RecordStage(65)
EndFunction

Function Fragment_Stage_0070_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    Alias_FinalExamRespawnTarget.Clear()
    If MTR06_TrainingMine != None && MTR06_TrainingMine.IsRunning()
        MTR06_TrainingMine.Stop()
    EndIf
    If playerRef != None
        Alias_PlayerCanRegister.ForceRefTo(playerRef)
        playerRef.SetValue(MTR06_CheckpointValue, 4.0)
    EndIf
    MTR06_RecordStage(70)
EndFunction

Function Fragment_Stage_0080_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
    Alias_PlayerCanRegister.Clear()
    If playerRef != None
        Alias_PlayerRegistered.ForceRefTo(playerRef)
        Alias_PlayerCanUseDispatch.ForceRefTo(playerRef)
        MTR06_GiveAliasItem(playerRef, Alias_OutroHolotape)
    EndIf
    MTR06_RecordStage(80)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(80)
    Alias_PlayerCanUseDispatch.Clear()
    If MTR06_Brigade_0090_MadiganRescue != None && !MTR06_Brigade_0090_MadiganRescue.IsPlaying()
        MTR06_Brigade_0090_MadiganRescue.Start()
    EndIf
    MTR06_RecordStage(90)
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = MTR06_GetPlayer()
    CompleteAllObjectives()
    If playerRef != None
        playerRef.SetValue(MTR06_QuestCompleted, 1.0)
    EndIf
    If MTR06_PostMisc_QuestStartKeyword != None
        MTR06_PostMisc_QuestStartKeyword.SendStoryEvent(akRef1 = playerRef)
    EndIf
    Bool nextQuestStarted = False
    If playerRef != None && MTN_MQ_Missing_Quest_Keyword != None
        nextQuestStarted = MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If nextQuestStarted && playerRef != None
        playerRef.SetValue(MTN_MQ_StartedValue, 1.0)
    EndIf
    MTR06_RecordStage(100)
EndFunction

Function Fragment_Stage_0999_Item_00()
    Alias_PlayerReadyForKnowledgeExam.Clear()
    Alias_PlayerReadyForPhysExam.Clear()
    Alias_PlayerReadyForFinalExam.Clear()
    Alias_PlayerCanCollectUniform.Clear()
    Alias_PlayerCanTriggerBeacon.Clear()
    Alias_PlayerCanRegister.Clear()
    Alias_PlayerCanUseDispatch.Clear()
    Alias_PlayerRegistered.Clear()
    Alias_FinalExamRespawnTarget.Clear()
    If MTR06_TrainingMine != None && MTR06_TrainingMine.IsRunning()
        MTR06_TrainingMine.Stop()
    EndIf
    If MTR06_Brigade_0035_FinalExamIntro != None && MTR06_Brigade_0035_FinalExamIntro.IsPlaying()
        MTR06_Brigade_0035_FinalExamIntro.Stop()
    EndIf
    If MTR06_Brigade_0090_MadiganRescue != None && MTR06_Brigade_0090_MadiganRescue.IsPlaying()
        MTR06_Brigade_0090_MadiganRescue.Stop()
    EndIf
EndFunction
