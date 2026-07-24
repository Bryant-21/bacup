; TODO

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    If !IsStageDone(15)
        SetStage(15)
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    ObjectReference actorEnableMarker = Alias_VaultTecUActorEnableMarker.GetReference()
    If actorEnableMarker
        actorEnableMarker.Enable()
    EndIf
    If W05_MQ_102p_NPCEnableMarker
        W05_MQ_102p_NPCEnableMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    If W05_MQ_102P_EnteredScene && !W05_MQ_102P_EnteredScene.IsPlaying()
        W05_MQ_102P_EnteredScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0530_Item_00()
    SetObjectiveDisplayed(530)
EndFunction

Function Fragment_Stage_0540_Item_00()
    SetObjectiveDisplayed(540)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveDisplayed(550)
EndFunction

Function Fragment_Stage_0580_Item_00()
    W05_MQ_102P_007a_ArrestBrass.Start()
EndFunction

Function Fragment_Stage_0584_Item_00()
    If IsStageDone(585)
        SetStage(586)
    EndIf
EndFunction

Function Fragment_Stage_0585_Item_00()
    If IsStageDone(584)
        SetStage(586)
    EndIf
EndFunction

Function Fragment_Stage_0586_Item_00()
    W05_MQ_102P_007c_DeathAftermath.Start()
EndFunction

Function Fragment_Stage_0590_Item_00()
    W05_MQ_102P_008a_BrassConfession.Start()
EndFunction

Function Fragment_Stage_0680_Item_00()
    W05_MQ_102P_007b_ArrestLoris.Start()
EndFunction

Function Fragment_Stage_0684_Item_00()
    If IsStageDone(685)
        SetStage(686)
    EndIf
EndFunction

Function Fragment_Stage_0685_Item_00()
    If IsStageDone(684)
        SetStage(686)
    EndIf
EndFunction

Function Fragment_Stage_0686_Item_00()
    W05_MQ_102P_007c_DeathAftermath.Start()
EndFunction

Function Fragment_Stage_0690_Item_00()
    W05_MQ_102P_008b_LorisConfession.Start()
EndFunction

Function Fragment_Stage_0730_Item_00()
    W05_MQ_102P_009a_EstellaReveal.Start()
EndFunction

Function Fragment_Stage_1300_Item_00()
    W05_MQ_102P_013_Vault79PresentationScene.Start()
EndFunction

Function Fragment_Stage_1400_Item_00()
    If !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQ_102P_A_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    W05_MQ_102P_B_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction

Function Fragment_Stage_1600_Item_00()
    If IsStageDone(1700) && !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    If IsStageDone(1600) && !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction
