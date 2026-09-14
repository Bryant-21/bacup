Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
        If playerRef != None
            Alias_Player.ForceRefTo(playerRef)
        EndIf
    EndIf
    If playerRef != None && pBoS02StartedAV != None
        playerRef.SetValue(pBoS02StartedAV, 1.0)
    EndIf
    If pBoS_Radio != None && pBoS_Radio.IsRunning()
        pBoS_Radio.Stop()
    EndIf
    If !IsObjectiveCompleted(100)
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && pBoS02StartedAV != None
        playerRef.SetValue(pBoS02StartedAV, 1.0)
    EndIf
    If GetStage() < 500
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsObjectiveCompleted(100)
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(150, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(150, True)
    SetObjectiveDisplayed(200, True)
    If pBoS02_200_DeniedEntry != None && !pBoS02_200_DeniedEntry.IsPlaying()
        pBoS02_200_DeniedEntry.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
    ObjectReference mapMarker = Alias_McClintockMap.GetReference()
    If mapMarker != None
        mapMarker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True)

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Bool hasCertificate = playerRef != None && pBoS02SoldierCertificate != None && playerRef.GetItemCount(pBoS02SoldierCertificate) > 0
    Bool trainingComplete = pEN05_Basic != None && pEN05_Basic.IsCompleted()
    If hasCertificate || trainingComplete
        SetStage(500)
    ElseIf playerRef != None && pEN05_Basic != None && !pEN05_Basic.IsRunning()
        Keyword startKeyword = Game.GetFormFromFile(0x00182072, "SeventySix.esm") as Keyword
        If startKeyword != None
            startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(500, True)
    ObjectReference charlestonMarker = Alias_CharlestonMapMarker.GetReference()
    If charlestonMarker != None
        charlestonMarker.AddToMap()
    EndIf
    If pBoS02DMVMarker != None
        pBoS02DMVMarker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Location selectedLocation = Alias_Q1_01.GetLocation()
    If selectedLocation != None
        Alias_Form_Name.ForceLocationTo(selectedLocation)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS02IDChoiceAV != None
        playerRef.SetValue(pBoS02IDChoiceAV, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    Location selectedLocation = Alias_Q1_02.GetLocation()
    If selectedLocation != None
        Alias_Form_Name.ForceLocationTo(selectedLocation)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS02IDChoiceAV != None
        playerRef.SetValue(pBoS02IDChoiceAV, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0530_Item_00()
    Location selectedLocation = Alias_Q1_03.GetLocation()
    If selectedLocation != None
        Alias_Form_Name.ForceLocationTo(selectedLocation)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS02IDChoiceAV != None
        playerRef.SetValue(pBoS02IDChoiceAV, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0540_Item_00()
    Location selectedLocation = Alias_Q1_04.GetLocation()
    If selectedLocation != None
        Alias_Form_Name.ForceLocationTo(selectedLocation)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS02IDChoiceAV != None
        playerRef.SetValue(pBoS02IDChoiceAV, 4.0)
    EndIf
EndFunction

Function Fragment_Stage_0551_Item_00()
    Location selectedLocation = Alias_Q2_01.GetLocation()
    If selectedLocation != None
        Alias_FormJob.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0552_Item_00()
    Location selectedLocation = Alias_Q2_02.GetLocation()
    If selectedLocation != None
        Alias_FormJob.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0553_Item_00()
    Location selectedLocation = Alias_Q2_03.GetLocation()
    If selectedLocation != None
        Alias_FormJob.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0554_Item_00()
    Location selectedLocation = Alias_Q2_04.GetLocation()
    If selectedLocation != None
        Alias_FormJob.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0561_Item_00()
    Location selectedLocation = Alias_Q3_01.GetLocation()
    If selectedLocation != None
        Alias_FormAddy.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0562_Item_00()
    Location selectedLocation = Alias_Q3_02.GetLocation()
    If selectedLocation != None
        Alias_FormAddy.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0563_Item_00()
    Location selectedLocation = Alias_Q3_03.GetLocation()
    If selectedLocation != None
        Alias_FormAddy.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0564_Item_00()
    Location selectedLocation = Alias_Q3_04.GetLocation()
    If selectedLocation != None
        Alias_FormAddy.ForceLocationTo(selectedLocation)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500, True)
    SetObjectiveDisplayed(600, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && pBoS02_ApplicationForm != None && playerRef.GetItemCount(pBoS02_ApplicationForm) == 0
        playerRef.AddItem(pBoS02_ApplicationForm, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(600, True)
    SetObjectiveDisplayed(800, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(800, True)
    SetObjectiveDisplayed(900, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(900, True)
    SetObjectiveDisplayed(1000, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS02_JunkMail != None && playerRef.GetItemCount(pBoS02_JunkMail) > 0
        playerRef.RemoveItem(pBoS02_JunkMail, playerRef.GetItemCount(pBoS02_JunkMail), True)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000, True)
    SetObjectiveDisplayed(1100, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && pBoS02_ApplicationFormV != None && playerRef.GetItemCount(pBoS02_ApplicationFormV) == 0
        playerRef.AddItem(pBoS02_ApplicationFormV, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1100, True)
    SetObjectiveDisplayed(1200, True)
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(1200, True)
    SetObjectiveDisplayed(1400, True)
EndFunction

Function Fragment_Stage_1450_Item_00()
    SetObjectiveCompleted(1400, True)
    SetObjectiveDisplayed(1450, True)
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(1450, True)
    SetObjectiveDisplayed(1500, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        If pBoS02_ApplicationFormV != None && playerRef.GetItemCount(pBoS02_ApplicationFormV) > 0
            playerRef.RemoveItem(pBoS02_ApplicationFormV, playerRef.GetItemCount(pBoS02_ApplicationFormV), True)
        EndIf
        If pBoS02_ApplicationFormStamped != None && playerRef.GetItemCount(pBoS02_ApplicationFormStamped) == 0
            playerRef.AddItem(pBoS02_ApplicationFormStamped, 1, False)
        EndIf
    EndIf
    ObjectReference amendmentBox = Alias_AmendmentBox.GetReference()
    If amendmentBox != None && pQSTBoS02AmendmentScribble != None
        pQSTBoS02AmendmentScribble.Play(amendmentBox)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(1500, True)
    SetObjectiveDisplayed(1600, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS02_ApplicationFormStamped != None && playerRef.GetItemCount(pBoS02_ApplicationFormStamped) > 0
        playerRef.RemoveItem(pBoS02_ApplicationFormStamped, playerRef.GetItemCount(pBoS02_ApplicationFormStamped), True)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(1600, True)
    SetObjectiveDisplayed(1700, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        If pBoS02GovernmentIDCard != None && playerRef.GetItemCount(pBoS02GovernmentIDCard) == 0
            playerRef.AddItem(pBoS02GovernmentIDCard, 1, False)
        EndIf
        If pBoS02MilitaryIDCard != None && playerRef.GetItemCount(pBoS02MilitaryIDCard) == 0
            playerRef.AddItem(pBoS02MilitaryIDCard, 1, False)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1750_Item_00()
    SetObjectiveCompleted(1700, True)
    SetObjectiveDisplayed(1800, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        Alias_PlayerHasAccess.ForceRefTo(playerRef)
    EndIf
    Alias_LaserGrid.TryToDisableNoWait()
    Alias_LaserGrid02.TryToDisableNoWait()
EndFunction

Function Fragment_Stage_1800_Item_00()
    If !IsObjectiveCompleted(1800)
        SetObjectiveDisplayed(1800, True)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(1800, True)
    CompleteAllObjectives()

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Bool checkpointAdvanced = False
    If playerRef != None
        If pBoS02CompletedAV != None
            playerRef.SetValue(pBoS02CompletedAV, 1.0)
        EndIf
        If pBoS02_CheckpointValue != None && playerRef.GetValue(pBoS02_CheckpointValue) < 1.0
            playerRef.SetValue(pBoS02_CheckpointValue, 1.0)
            checkpointAdvanced = True
        EndIf
        If pBoSTechnicalDocument != None && playerRef.GetItemCount(pBoSTechnicalDocument) == 0
            playerRef.AddItem(pBoSTechnicalDocument, 1, False)
        EndIf
    EndIf
    If checkpointAdvanced && pCheckpointMessage != None
        pCheckpointMessage.Show()
    EndIf

    CompleteQuest()

    If playerRef != None && pBoSZ01_QuestStartKeyword != None
        pBoSZ01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If !BoS02_TryStartBoS03()
        StartTimer(5.0, 1900)
    EndIf
EndFunction

Bool Function BoS02_TryStartBoS03()
    If pBoS03 != None && (pBoS03.IsRunning() || pBoS03.IsCompleted())
        Return True
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None || pBoS03_QuestStartKeyword == None
        Return False
    EndIf

    Bool accepted = pBoS03_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    Bool started = accepted || (pBoS03 != None && (pBoS03.IsRunning() || pBoS03.IsCompleted()))
    If started && pBoS03StartedAV != None
        playerRef.SetValue(pBoS03StartedAV, 1.0)
    EndIf
    Return started
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1900 || !GetStageDone(1900)
        Return
    EndIf

    If !BoS02_TryStartBoS03()
        StartTimer(5.0, 1900)
    EndIf
EndEvent
