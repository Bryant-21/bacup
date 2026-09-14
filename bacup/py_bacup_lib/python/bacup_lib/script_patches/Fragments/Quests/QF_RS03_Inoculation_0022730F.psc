Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None
        Return
    EndIf
    If RS03_Inoculation_Started != None
        playerRef.SetValue(RS03_Inoculation_Started, 1.0)
    EndIf

    Int bloodCheckpoint = 0
    If RS03_Inoculation_CheckPointBlood != None
        bloodCheckpoint = playerRef.GetValue(RS03_Inoculation_CheckPointBlood) as Int
    EndIf
    Int fuseCheckpoint = 0
    If RS03_Inoculation_CheckPointFuse != None
        fuseCheckpoint = playerRef.GetValue(RS03_Inoculation_CheckPointFuse) as Int
    EndIf

    If bloodCheckpoint <= 0 && fuseCheckpoint <= 0
        If !IsStageDone(100)
            SetStage(100)
        EndIf
    ElseIf !IsStageDone(200)
        SetStage(200)
    EndIf

    RestoreCanonicalSampleHistory(bloodCheckpoint)
    RestoreCanonicalFuseHistory(fuseCheckpoint)
    If bloodCheckpoint >= CPBlood_SamplesInCentrifuge && !IsStageDone(500)
        SetStage(500)
    ElseIf IsStageDone(315) && IsStageDone(260) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function RestoreCanonicalSampleHistory(Int bloodCheckpoint)
    Bool hasGhoulSample = bloodCheckpoint == CPBlood_GhoulOnly || bloodCheckpoint == CPBlood_GhoulAndMoleRat || bloodCheckpoint == CPBlood_GhoulAndWolf || bloodCheckpoint >= CPBlood_AllSamplesCollected
    If hasGhoulSample && !IsStageDone(315)
        SetStage(315)
    EndIf
EndFunction

Function RestoreCanonicalFuseHistory(Int fuseCheckpoint)
    If fuseCheckpoint >= CPFuse_FuseCollectedNotInstalled && !IsStageDone(250)
        SetStage(250)
    EndIf
    If fuseCheckpoint >= CPFuse_FuseInstalled
        If !IsStageDone(260)
            SetStage(260)
        EndIf
    ElseIf fuseCheckpoint >= CPFuse_FuseCollectedNotInstalled
        If !IsStageDone(760)
            SetStage(760)
        EndIf
    ElseIf !IsStageDone(210)
        SetStage(210)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    If !IsStageDone(205)
        SetStage(205)
    EndIf
    If !IsStageDone(210)
        SetStage(210)
    EndIf
EndFunction

Function Fragment_Stage_0205_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    PrepareModernSampleCollection()
    If playerRef != None
        If RS03_Inoculation_CheckPointBlood != None && playerRef.GetValue(RS03_Inoculation_CheckPointBlood) < CPBlood_NoSamplesCollected
            playerRef.SetValue(RS03_Inoculation_CheckPointBlood, CPBlood_NoSamplesCollected as Float)
        EndIf
    EndIf
EndFunction

Function PrepareModernSampleCollection()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveDisplayed(25, False)
    SetObjectiveDisplayed(45, False)
    SetObjectiveDisplayed(35, True)
    If playerRef != None
        If RS03_MoleratBloodSamplePerk != None && playerRef.HasPerk(RS03_MoleratBloodSamplePerk)
            playerRef.RemovePerk(RS03_MoleratBloodSamplePerk)
        EndIf
        If RS03_WolfBloodSamplePerk != None && playerRef.HasPerk(RS03_WolfBloodSamplePerk)
            playerRef.RemovePerk(RS03_WolfBloodSamplePerk)
        EndIf
        If RS03_GhoulBloodSamplePerk != None && !playerRef.HasPerk(RS03_GhoulBloodSamplePerk)
            playerRef.AddPerk(RS03_GhoulBloodSamplePerk)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveDisplayed(12, True)
    SetObjectiveDisplayed(15, True)
    If playerRef != None
        If RS03_Inoculation_CanUseFusebox != None
            playerRef.SetValue(RS03_Inoculation_CanUseFusebox, 1.0)
        EndIf
        If RS03_Inoculation_CheckPointFuse != None && playerRef.GetValue(RS03_Inoculation_CheckPointFuse) < CPFuse_FuseNotCollected
            playerRef.SetValue(RS03_Inoculation_CheckPointFuse, CPFuse_FuseNotCollected as Float)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetObjectiveDisplayed(12, True, True)
EndFunction

Function Fragment_Stage_0230_Item_00()
    SetObjectiveDisplayed(12, True, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(12, True)
    SetObjectiveCompleted(15, True)
    SetObjectiveDisplayed(17, True)
    If playerRef != None && RS03_Inoculation_CheckPointFuse != None && playerRef.GetValue(RS03_Inoculation_CheckPointFuse) < CPFuse_FuseCollectedNotInstalled
        playerRef.SetValue(RS03_Inoculation_CheckPointFuse, CPFuse_FuseCollectedNotInstalled as Float)
    EndIf
EndFunction

Function Fragment_Stage_0260_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(12, True)
    SetObjectiveCompleted(15, True)
    SetObjectiveCompleted(17, True)
    If playerRef != None
        If RS03_Inoculation_TypeTFuse != None && playerRef.GetItemCount(RS03_Inoculation_TypeTFuse) > 0
            playerRef.RemoveItem(RS03_Inoculation_TypeTFuse, 1, True)
        EndIf
        If RS03_Inoculation_CanUseFusebox != None
            playerRef.SetValue(RS03_Inoculation_CanUseFusebox, 0.0)
        EndIf
        If RS03_Inoculation_CheckPointFuse != None
            playerRef.SetValue(RS03_Inoculation_CheckPointFuse, CPFuse_FuseInstalled as Float)
        EndIf
    EndIf
    If IsStageDone(315) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0305_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(25, True)
    If playerRef != None
        If RS03_Inoculation_MoleratBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_MoleratBloodSample) == 0
            playerRef.AddItem(RS03_Inoculation_MoleratBloodSample, 1, False)
        EndIf
        If RS03_MoleratBloodSamplePerk != None && playerRef.HasPerk(RS03_MoleratBloodSamplePerk)
            playerRef.RemovePerk(RS03_MoleratBloodSamplePerk)
        EndIf
        BloodSamplesCollected = 1
        Int checkpointValue = CPBlood_MoleratOnly
        If IsStageDone(315) && IsStageDone(325)
            BloodSamplesCollected = 3
            checkpointValue = CPBlood_AllSamplesCollected
        ElseIf IsStageDone(315)
            BloodSamplesCollected = 2
            checkpointValue = CPBlood_GhoulAndMoleRat
        ElseIf IsStageDone(325)
            BloodSamplesCollected = 2
            checkpointValue = CPBlood_MoleRatAndWolf
        EndIf
        If RS03_Inoculation_CheckPointBlood != None && playerRef.GetValue(RS03_Inoculation_CheckPointBlood) < checkpointValue
            playerRef.SetValue(RS03_Inoculation_CheckPointBlood, checkpointValue as Float)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0315_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveDisplayed(25, False)
    SetObjectiveCompleted(35, True)
    SetObjectiveDisplayed(45, False)
    If playerRef != None
        If RS03_Inoculation_GhoulBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_GhoulBloodSample) == 0
            playerRef.AddItem(RS03_Inoculation_GhoulBloodSample, 1, False)
        EndIf
        If RS03_GhoulBloodSamplePerk != None && playerRef.HasPerk(RS03_GhoulBloodSamplePerk)
            playerRef.RemovePerk(RS03_GhoulBloodSamplePerk)
        EndIf
        BloodSamplesCollected = 1
        Int checkpointValue = CPBlood_GhoulOnly
        If IsStageDone(305) && IsStageDone(325)
            BloodSamplesCollected = 3
            checkpointValue = CPBlood_AllSamplesCollected
        ElseIf IsStageDone(305)
            BloodSamplesCollected = 2
            checkpointValue = CPBlood_GhoulAndMoleRat
        ElseIf IsStageDone(325)
            BloodSamplesCollected = 2
            checkpointValue = CPBlood_GhoulAndWolf
        EndIf
        If RS03_Inoculation_CheckPointBlood != None && playerRef.GetValue(RS03_Inoculation_CheckPointBlood) < checkpointValue
            playerRef.SetValue(RS03_Inoculation_CheckPointBlood, checkpointValue as Float)
        EndIf
    EndIf
    If IsStageDone(260) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(45, True)
    If playerRef != None
        If RS03_Inoculation_WolfBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_WolfBloodSample) == 0
            playerRef.AddItem(RS03_Inoculation_WolfBloodSample, 1, False)
        EndIf
        If RS03_WolfBloodSamplePerk != None && playerRef.HasPerk(RS03_WolfBloodSamplePerk)
            playerRef.RemovePerk(RS03_WolfBloodSamplePerk)
        EndIf
        BloodSamplesCollected = 1
        Int checkpointValue = CPBlood_WolfOnly
        If IsStageDone(305) && IsStageDone(315)
            BloodSamplesCollected = 3
            checkpointValue = CPBlood_AllSamplesCollected
        ElseIf IsStageDone(305)
            BloodSamplesCollected = 2
            checkpointValue = CPBlood_MoleRatAndWolf
        ElseIf IsStageDone(315)
            BloodSamplesCollected = 2
            checkpointValue = CPBlood_GhoulAndWolf
        EndIf
        If RS03_Inoculation_CheckPointBlood != None && playerRef.GetValue(RS03_Inoculation_CheckPointBlood) < checkpointValue
            playerRef.SetValue(RS03_Inoculation_CheckPointBlood, checkpointValue as Float)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(25, True)
    SetObjectiveCompleted(35, True)
    SetObjectiveCompleted(45, True)
    SetObjectiveDisplayed(60, True)
    If playerRef != None && RS03_Inoculation_CanUseCentrifuge != None
        playerRef.SetValue(RS03_Inoculation_CanUseCentrifuge, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(70, True)
    If playerRef != None
        If RS03_Inoculation_CanUseCentrifuge != None
            playerRef.SetValue(RS03_Inoculation_CanUseCentrifuge, 0.0)
        EndIf
        If RS03_Inoculation_CheckPointBlood != None
            playerRef.SetValue(RS03_Inoculation_CheckPointBlood, CPBlood_SamplesInCentrifuge as Float)
        EndIf
        If RS03_Inoculation_MoleratBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_MoleratBloodSample) > 0
            playerRef.RemoveItem(RS03_Inoculation_MoleratBloodSample, 1, True)
        EndIf
        If RS03_Inoculation_GhoulBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_GhoulBloodSample) > 0
            playerRef.RemoveItem(RS03_Inoculation_GhoulBloodSample, 1, True)
        EndIf
        If RS03_Inoculation_WolfBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_WolfBloodSample) > 0
            playerRef.RemoveItem(RS03_Inoculation_WolfBloodSample, 1, True)
        EndIf
    EndIf
    ObjectReference centrifugeRef = Alias_Centrifuge.GetReference()
    If OBJBeakerInsertOneshot != None && centrifugeRef != None
        OBJBeakerInsertOneshot.Play(centrifugeRef)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(72, True)
    If OBJMixerMachineOneshotComp != None && RS03_Inoculation_CentrifugeSoundMarker != None
        OBJMixerMachineOneshotComp.Play(RS03_Inoculation_CentrifugeSoundMarker)
    EndIf
EndFunction

Function Fragment_Stage_0575_Item_00()
    SetObjectiveCompleted(72, True)
    SetObjectiveDisplayed(75, True)
    If RS03_Inoculation_CentrifugeSoundMarker != None
        RS03_Inoculation_CentrifugeSoundMarker.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(70, True)
    SetObjectiveCompleted(72, True)
    SetObjectiveCompleted(75, True)
    SetObjectiveDisplayed(80, True)
    If RS03_Inoculation_CentrifugeSoundMarker != None
        RS03_Inoculation_CentrifugeSoundMarker.Disable()
    EndIf
    If playerRef != None && RS03_Inoculation_CanUseSymptomatic != None
        playerRef.SetValue(RS03_Inoculation_CanUseSymptomatic, 1.0)
    EndIf
    If RS03_Inoculation_SymptomaticSpeakerScene != None && !RS03_Inoculation_SymptomaticSpeakerScene.IsPlaying()
        RS03_Inoculation_SymptomaticSpeakerScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    PrepareModernSampleCollection()
EndFunction

Function Fragment_Stage_0700_Item_00()
    PrepareModernSampleCollection()
EndFunction

Function Fragment_Stage_0710_Item_00()
    If !IsStageDone(315)
        SetStage(315)
    EndIf
EndFunction

Function Fragment_Stage_0720_Item_00()
    PrepareModernSampleCollection()
EndFunction

Function Fragment_Stage_0730_Item_00()
    If !IsStageDone(315)
        SetStage(315)
    EndIf
EndFunction

Function Fragment_Stage_0740_Item_00()
    If !IsStageDone(315)
        SetStage(315)
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    PrepareModernSampleCollection()
EndFunction

Function Fragment_Stage_0760_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(12, True)
    SetObjectiveCompleted(15, True)
    SetObjectiveDisplayed(17, True)
    If playerRef != None
        If RS03_Inoculation_TypeTFuse != None && playerRef.GetItemCount(RS03_Inoculation_TypeTFuse) == 0
            playerRef.AddItem(RS03_Inoculation_TypeTFuse, 1, False)
        EndIf
        If RS03_Inoculation_CanUseFusebox != None
            playerRef.SetValue(RS03_Inoculation_CanUseFusebox, 1.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0770_Item_00()
    If !IsStageDone(315)
        SetStage(315)
    EndIf
    If IsStageDone(260) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    SetObjectiveCompleted(80, True)
    If playerRef != None
        If RS03_Inoculation_InoculationSpell != None && !playerRef.HasSpell(RS03_Inoculation_InoculationSpell)
            playerRef.AddSpell(RS03_Inoculation_InoculationSpell, False)
        EndIf
        If RS03_Inoculation_Completed != None
            playerRef.SetValue(RS03_Inoculation_Completed, 1.0)
        EndIf
        If RS03_Inoculation_CanUseSymptomatic != None
            playerRef.SetValue(RS03_Inoculation_CanUseSymptomatic, 0.0)
        EndIf
        If RS03_MoleratBloodSamplePerk != None && playerRef.HasPerk(RS03_MoleratBloodSamplePerk)
            playerRef.RemovePerk(RS03_MoleratBloodSamplePerk)
        EndIf
        If RS03_GhoulBloodSamplePerk != None && playerRef.HasPerk(RS03_GhoulBloodSamplePerk)
            playerRef.RemovePerk(RS03_GhoulBloodSamplePerk)
        EndIf
        If RS03_WolfBloodSamplePerk != None && playerRef.HasPerk(RS03_WolfBloodSamplePerk)
            playerRef.RemovePerk(RS03_WolfBloodSamplePerk)
        EndIf
        If RS03_Inoculation_Message != None
            RS03_Inoculation_Message.Show()
        EndIf
        If MTR06_TriggeredMiscValue != None
            playerRef.SetValue(MTR06_TriggeredMiscValue, 1.0)
        EndIf
        If MTR06_QuestStarted == None || playerRef.GetValue(MTR06_QuestStarted) < 1.0
            If MTR06_QuestStartKeyword != None
                MTR06_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
            EndIf
        EndIf
        If W05_MQ_101P_Started != None && playerRef.GetValue(W05_MQ_101P_Started) > 0.0 && W05_MQ_101P_A_Started != None && playerRef.GetValue(W05_MQ_101P_A_Started) < 1.0
            If W05_MQ_101P_QuestStartKeyword != None
                W05_MQ_101P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
            EndIf
        EndIf
    EndIf
    If !IsStageDone(1100)
        SetStage(1100)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_2000_Item_00()
    Actor playerRef = Alias_RS03_Inoculation_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        If RS03_Inoculation_TypeTFuse != None && playerRef.GetItemCount(RS03_Inoculation_TypeTFuse) > 0
            playerRef.RemoveItem(RS03_Inoculation_TypeTFuse, playerRef.GetItemCount(RS03_Inoculation_TypeTFuse), True)
        EndIf
        If RS03_Inoculation_MoleratBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_MoleratBloodSample) > 0
            playerRef.RemoveItem(RS03_Inoculation_MoleratBloodSample, playerRef.GetItemCount(RS03_Inoculation_MoleratBloodSample), True)
        EndIf
        If RS03_Inoculation_GhoulBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_GhoulBloodSample) > 0
            playerRef.RemoveItem(RS03_Inoculation_GhoulBloodSample, playerRef.GetItemCount(RS03_Inoculation_GhoulBloodSample), True)
        EndIf
        If RS03_Inoculation_WolfBloodSample != None && playerRef.GetItemCount(RS03_Inoculation_WolfBloodSample) > 0
            playerRef.RemoveItem(RS03_Inoculation_WolfBloodSample, playerRef.GetItemCount(RS03_Inoculation_WolfBloodSample), True)
        EndIf
        If RS03_MoleratBloodSamplePerk != None && playerRef.HasPerk(RS03_MoleratBloodSamplePerk)
            playerRef.RemovePerk(RS03_MoleratBloodSamplePerk)
        EndIf
        If RS03_GhoulBloodSamplePerk != None && playerRef.HasPerk(RS03_GhoulBloodSamplePerk)
            playerRef.RemovePerk(RS03_GhoulBloodSamplePerk)
        EndIf
        If RS03_WolfBloodSamplePerk != None && playerRef.HasPerk(RS03_WolfBloodSamplePerk)
            playerRef.RemovePerk(RS03_WolfBloodSamplePerk)
        EndIf
    EndIf
    Stop()
EndFunction
