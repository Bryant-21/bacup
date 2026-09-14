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
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    If W05_MQ_102P_EnteredScene && !W05_MQ_102P_EnteredScene.IsPlaying()
        W05_MQ_102P_EnteredScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_0530_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0540_Item_00()
    SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0560_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_102P_VTec_Holotape02) == 0
        playerRef.AddItem(W05_MQ_102P_VTec_Holotape02, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0565_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    Actor chiefRef = Alias_ChiefEngineer.GetActorReference()
    If chiefRef
        chiefRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0580_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(400)
    If W05_MQ_102P_007a_ArrestBrass && !W05_MQ_102P_007a_ArrestBrass.IsPlaying()
        W05_MQ_102P_007a_ArrestBrass.Start()
    EndIf
EndFunction

Function Fragment_Stage_0582_Item_00()
    SetObjectiveDisplayed(410)
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    Int engineerIndex = 0
    While playerRef && engineerIndex < Alias_Engineers.GetCount()
        Actor engineerRef = Alias_Engineers.GetAt(engineerIndex) as Actor
        If engineerRef && !engineerRef.IsDead()
            engineerRef.StartCombat(playerRef)
        EndIf
        engineerIndex += 1
    EndWhile
EndFunction

Function Fragment_Stage_0584_Item_00()
    If IsStageDone(585) && !IsStageDone(586)
        SetStage(586)
    EndIf
EndFunction

Function Fragment_Stage_0585_Item_00()
    If IsStageDone(584) && !IsStageDone(586)
        SetStage(586)
    EndIf
EndFunction

Function Fragment_Stage_0586_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(410)
    If W05_MQ_102P_007c_DeathAftermath && !W05_MQ_102P_007c_DeathAftermath.IsPlaying()
        W05_MQ_102P_007c_DeathAftermath.Start()
    EndIf
EndFunction

Function Fragment_Stage_0590_Item_00()
    SetObjectiveCompleted(400)
    If W05_MQ_102P_008a_BrassConfession && !W05_MQ_102P_008a_BrassConfession.IsPlaying()
        W05_MQ_102P_008a_BrassConfession.Start()
    EndIf
EndFunction

Function Fragment_Stage_0595_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(250)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0610_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(300)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_102P_ReactorKey) == 0
        playerRef.AddItem(W05_MQ_102P_ReactorKey, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0615_Item_00()
    SetObjectiveDisplayed(250)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0630_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(200)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_102P_LorisNote) == 0
        playerRef.AddItem(W05_MQ_102P_LorisNote, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0640_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(210)
EndFunction

Function Fragment_Stage_0665_Item_00()
    SetObjectiveCompleted(210)
    SetObjectiveDisplayed(200)
    Actor chiefRef = Alias_ChiefEngineer.GetActorReference()
    If chiefRef
        chiefRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0680_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(400)
    If W05_MQ_102P_007b_ArrestLoris && !W05_MQ_102P_007b_ArrestLoris.IsPlaying()
        W05_MQ_102P_007b_ArrestLoris.Start()
    EndIf
EndFunction

Function Fragment_Stage_0682_Item_00()
    SetObjectiveDisplayed(420)
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    Int staffIndex = 0
    While playerRef && staffIndex < Alias_MedStaff.GetCount()
        Actor staffRef = Alias_MedStaff.GetAt(staffIndex) as Actor
        If staffRef && !staffRef.IsDead()
            staffRef.StartCombat(playerRef)
        EndIf
        staffIndex += 1
    EndWhile
EndFunction

Function Fragment_Stage_0684_Item_00()
    If IsStageDone(685) && !IsStageDone(686)
        SetStage(686)
    EndIf
EndFunction

Function Fragment_Stage_0685_Item_00()
    If IsStageDone(684) && !IsStageDone(686)
        SetStage(686)
    EndIf
EndFunction

Function Fragment_Stage_0686_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(420)
    If W05_MQ_102P_007c_DeathAftermath && !W05_MQ_102P_007c_DeathAftermath.IsPlaying()
        W05_MQ_102P_007c_DeathAftermath.Start()
    EndIf
EndFunction

Function Fragment_Stage_0690_Item_00()
    SetObjectiveCompleted(400)
    If W05_MQ_102P_008b_LorisConfession && !W05_MQ_102P_008b_LorisConfession.IsPlaying()
        W05_MQ_102P_008b_LorisConfession.Start()
    EndIf
EndFunction

Function Fragment_Stage_0695_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(250)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(250)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0710_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(310)
EndFunction

Function Fragment_Stage_0720_Item_00()
    SetObjectiveCompleted(310)
    SetObjectiveDisplayed(400)
    Actor chiefRef = Alias_ChiefEngineer.GetActorReference()
    If chiefRef
        chiefRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0730_Item_00()
    SetObjectiveCompleted(310)
    SetObjectiveDisplayed(400)
    If W05_MQ_102P_009a_EstellaReveal && !W05_MQ_102P_009a_EstellaReveal.IsPlaying()
        W05_MQ_102P_009a_EstellaReveal.Start()
    EndIf
EndFunction

Function Fragment_Stage_0740_Item_00()
    SetObjectiveCompleted(400)
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(410)
    SetObjectiveCompleted(420)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0850_Item_00()
    Actor overseerRef = Alias_OverseerVTec.GetActorReference()
    ObjectReference professorMarker = Alias_OverseerProfessorTeleportMarker.GetReference()
    If overseerRef && professorMarker
        overseerRef.MoveTo(professorMarker)
        overseerRef.EvaluatePackage()
    EndIf
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(510)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(510)
    If W05_MQ_102P_012a_Maintenance && !W05_MQ_102P_012a_Maintenance.IsPlaying()
        W05_MQ_102P_012a_Maintenance.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(510)
    SetObjectiveDisplayed(520)
    If W05_MQ_102P_012_PresentationRoom && !W05_MQ_102P_012_PresentationRoom.IsPlaying()
        W05_MQ_102P_012_PresentationRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(510)
    SetObjectiveDisplayed(520)
    If W05_MQ_102P_013_Vault79PresentationScene && !W05_MQ_102P_013_Vault79PresentationScene.IsPlaying()
        W05_MQ_102P_013_Vault79PresentationScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(520)
    SetObjectiveDisplayed(530)
    If !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(530)
    SetObjectiveDisplayed(540)
    SetObjectiveDisplayed(550)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If W05_MQ_102P_A && !W05_MQ_102P_A.IsRunning() && !W05_MQ_102P_A.IsCompleted()
        W05_MQ_102P_A_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    If W05_MQ_102P_B && !W05_MQ_102P_B.IsRunning() && !W05_MQ_102P_B.IsCompleted()
        W05_MQ_102P_B_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(540)
    If IsStageDone(1700) && !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(550)
    If IsStageDone(1600) && !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(540)
    SetObjectiveCompleted(550)
    If Vault79MapMarker
        Vault79MapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    If W05_MQ_102P_EnteredScene && W05_MQ_102P_EnteredScene.IsPlaying()
        W05_MQ_102P_EnteredScene.Stop()
    EndIf
    If W05_MQ_102P_012a_Maintenance && W05_MQ_102P_012a_Maintenance.IsPlaying()
        W05_MQ_102P_012a_Maintenance.Stop()
    EndIf
    If W05_MQ_102P_012_PresentationRoom && W05_MQ_102P_012_PresentationRoom.IsPlaying()
        W05_MQ_102P_012_PresentationRoom.Stop()
    EndIf
EndFunction
