Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Alias_Player != None
        Alias_Player.ForceRefIfEmpty(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0160_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_LowerLarge_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_LowerLarge_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_LowerLarge_Message != None
        BS01_MQ02_Invention_ValdezRep_LowerLarge_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
    If playerRef != None
        If BS01_ValdezAwayValue != None
            playerRef.SetValue(BS01_ValdezAwayValue, 1.0)
        EndIf
        If BS02_ArtKnappAwayValue != None
            playerRef.SetValue(BS02_ArtKnappAwayValue, 1.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    ObjectReference ambientMarker = Alias_AmbientEnemies_EnableMarker.GetReference()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
    If ambientMarker != None
        ambientMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    ObjectReference valdezRef = Alias_Valdez_Dungeon_Ref.GetReference()
    ObjectReference waitMarker = Alias_ValdezWaitMarker01_XMarkerHeading.GetReference()
    If valdezRef != None
        valdezRef.Enable()
        If waitMarker != None
            valdezRef.MoveTo(waitMarker)
        EndIf
    EndIf
    If BS01_Invention_Valdez_InitialTravel_Scene != None && !BS01_Invention_Valdez_InitialTravel_Scene.IsPlaying()
        BS01_Invention_Valdez_InitialTravel_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0410_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_LowerSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_LowerSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_LowerSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_LowerSmall_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_0430_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Valdez_Terminal_Password != None && playerRef.GetItemCount(BS01_Valdez_Terminal_Password) == 0
        playerRef.AddItem(BS01_Valdez_Terminal_Password, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0501_Item_00()
    If BS01_MQ02_Invention_Valdez_Hammond_Scene != None && !BS01_MQ02_Invention_Valdez_Hammond_Scene.IsPlaying()
        BS01_MQ02_Invention_Valdez_Hammond_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0505_Item_00()
    If BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene != None && !BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.IsPlaying()
        BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Int documentCount = 1
    If IsStageDone(520)
        documentCount += 1
    EndIf
    If IsStageDone(530)
        documentCount += 1
    EndIf
    If BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(documentCount * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0515_Item_00()
    If BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene != None && !BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.IsPlaying()
        BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    Int documentCount = 1
    If IsStageDone(510)
        documentCount += 1
    EndIf
    If IsStageDone(530)
        documentCount += 1
    EndIf
    If BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(documentCount * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0525_Item_00()
    If BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene != None && !BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.IsPlaying()
        BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0530_Item_00()
    Int documentCount = 1
    If IsStageDone(510)
        documentCount += 1
    EndIf
    If IsStageDone(520)
        documentCount += 1
    EndIf
    If BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(documentCount * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0610_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None
        If BS01_Invention_Documentation01 != None
            playerRef.RemoveItem(BS01_Invention_Documentation01, playerRef.GetItemCount(BS01_Invention_Documentation01), True)
        EndIf
        If BS01_Invention_Documentation02 != None
            playerRef.RemoveItem(BS01_Invention_Documentation02, playerRef.GetItemCount(BS01_Invention_Documentation02), True)
        EndIf
        If BS01_Invention_Documentation03 != None
            playerRef.RemoveItem(BS01_Invention_Documentation03, playerRef.GetItemCount(BS01_Invention_Documentation03), True)
        EndIf
    EndIf
    If BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene != None && !BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.IsPlaying()
        BS01_MQ02_Invention_Valdez_DiscussDocsQuip_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseLarge_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseLarge_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseLarge_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseLarge_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_0630_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_0640_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_LowerSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_LowerSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_LowerSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_LowerSmall_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
    SetObjectiveDisplayed(900)
    SetObjectiveDisplayed(1000)
    If BS01_Invention_Valdez_ExamineMachine_Scene != None && !BS01_Invention_Valdez_ExamineMachine_Scene.IsPlaying()
        BS01_Invention_Valdez_ExamineMachine_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    SetObjectiveCompleted(800)
    If BS01_Invention_Valdez_Diagnostics_Scene != None && !BS01_Invention_Valdez_Diagnostics_Scene.IsPlaying()
        BS01_Invention_Valdez_Diagnostics_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0720_Item_00()
    ObjectReference valveRef = Alias_ReleaseValve_Activator.GetReference()
    SetObjectiveCompleted(900)
    If valveRef != None && ReleaseValveExplosion != None
        valveRef.PlaceAtMe(ReleaseValveExplosion)
    EndIf
    If BS01_Invention_Valdez_ReleaseValve_Scene != None && !BS01_Invention_Valdez_ReleaseValve_Scene.IsPlaying()
        BS01_Invention_Valdez_ReleaseValve_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0730_Item_00()
    SetObjectiveCompleted(1000)
    If BS01_Invention_Valdez_Wiring_Scene != None && !BS01_Invention_Valdez_Wiring_Scene.IsPlaying()
        BS01_Invention_Valdez_Wiring_Scene.Start()
    EndIf
    If !IsStageDone(740)
        SetStage(740)
    EndIf
EndFunction

Function Fragment_Stage_0740_Item_00()
    ObjectReference spawnPoint = Alias_WiringFight_SpawnCenterMarker.GetReference()
    Actor playerRef = Alias_Player.GetActorReference()
    RefCollectionAlias wiringEnemies = GetAlias(35) as RefCollectionAlias
    ActorBase moleRatBase = Game.GetFormFromFile(0x000342C9, "Fallout4.esm") as ActorBase
    If spawnPoint == None
        spawnPoint = playerRef
    EndIf
    If spawnPoint != None
        spawnPoint.Enable()
        spawnPoint.Activate(playerRef)
    EndIf
    If spawnPoint != None && playerRef != None && wiringEnemies != None && wiringEnemies.GetCount() == 0 && moleRatBase != None
        Int spawnIndex = 0
        While spawnIndex < 4
            Actor enemyRef = spawnPoint.PlaceActorAtMe(moleRatBase, 3)
            If enemyRef != None
                wiringEnemies.AddRef(enemyRef)
                enemyRef.StartCombat(playerRef)
            EndIf
            spawnIndex += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    If IsStageDone(710) && IsStageDone(720) && IsStageDone(730) && !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(800)
    SetObjectiveCompleted(900)
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1100)
    If BS01_MQ02_Invention_Valdez_DiscussInspectionQuip_Scene != None && !BS01_MQ02_Invention_Valdez_DiscussInspectionQuip_Scene.IsPlaying()
        BS01_MQ02_Invention_Valdez_DiscussInspectionQuip_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message.Show()
    EndIf
    If (IsStageDone(810) || IsStageDone(820)) && (IsStageDone(830) || IsStageDone(840)) && (IsStageDone(850) || IsStageDone(860)) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_LowerSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_LowerSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_LowerSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_LowerSmall_Message.Show()
    EndIf
    If (IsStageDone(810) || IsStageDone(820)) && (IsStageDone(830) || IsStageDone(840)) && (IsStageDone(850) || IsStageDone(860)) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message.Show()
    EndIf
    If (IsStageDone(810) || IsStageDone(820)) && (IsStageDone(830) || IsStageDone(840)) && (IsStageDone(850) || IsStageDone(860)) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0840_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_LowerSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_LowerSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_LowerSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_LowerSmall_Message.Show()
    EndIf
    If (IsStageDone(810) || IsStageDone(820)) && (IsStageDone(830) || IsStageDone(840)) && (IsStageDone(850) || IsStageDone(860)) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0850_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseSmall_Message.Show()
    EndIf
    If (IsStageDone(810) || IsStageDone(820)) && (IsStageDone(830) || IsStageDone(840)) && (IsStageDone(850) || IsStageDone(860)) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0860_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_LowerSmall_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_LowerSmall_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_LowerSmall_Message != None
        BS01_MQ02_Invention_ValdezRep_LowerSmall_Message.Show()
    EndIf
    If (IsStageDone(810) || IsStageDone(820)) && (IsStageDone(830) || IsStageDone(840)) && (IsStageDone(850) || IsStageDone(860)) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1200)
    If BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(0.0)
    EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_CPU_XMarker.GetReference()
    ObjectReference componentRef = Alias_CoreProcessingUnit.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_CoreProcessingUnit_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_CoreProcessingUnit_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_CoreProcessingUnit.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullyEjectSound != None
        ExtractedSuccessfullyEjectSound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0911_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_CPU_XMarker.GetReference()
    ObjectReference componentRef = Alias_CoreProcessingUnit.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_CoreProcessingUnit_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_CoreProcessingUnit_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_CoreProcessingUnit.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullyEjectSound != None
        ExtractedSuccessfullyEjectSound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0912_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_CPU_XMarker.GetReference()
    ObjectReference componentRef = Alias_CoreProcessingUnit.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_CoreProcessingUnit_Damaged_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_CoreProcessingUnit_Damaged_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_CoreProcessingUnit.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedDamagedSound != None
        ExtractedDamagedSound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0913_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_CPU_XMarker.GetReference()
    ObjectReference componentRef = Alias_CoreProcessingUnit.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_CoreProcessingUnit_Destroyed_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_CoreProcessingUnit_Destroyed_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_CoreProcessingUnit.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None
        If ExtractedDestroyedSound != None
            ExtractedDestroyedSound.Play(markerRef)
        EndIf
        If ExtractedExplosion != None
            markerRef.PlaceAtMe(ExtractedExplosion)
        EndIf
    EndIf
    If playerRef != None && StaggerSpell != None
        StaggerSpell.Cast(playerRef, playerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0920_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_IonAccelerator_XMarker.GetReference()
    ObjectReference componentRef = Alias_IonAccelerator.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_IonAccelerator_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_IonAccelerator_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_IonAccelerator.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullySound != None
        ExtractedSuccessfullySound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0921_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_IonAccelerator_XMarker.GetReference()
    ObjectReference componentRef = Alias_IonAccelerator.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_IonAccelerator_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_IonAccelerator_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_IonAccelerator.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullySound != None
        ExtractedSuccessfullySound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0922_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_IonAccelerator_XMarker.GetReference()
    ObjectReference componentRef = Alias_IonAccelerator.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_IonAccelerator_Damaged_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_IonAccelerator_Damaged_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_IonAccelerator.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedDamagedSound != None
        ExtractedDamagedSound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0923_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_IonAccelerator_XMarker.GetReference()
    ObjectReference componentRef = Alias_IonAccelerator.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_IonAccelerator_Destroyed_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_IonAccelerator_Destroyed_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_IonAccelerator.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None
        If ExtractedDestroyedSound != None
            ExtractedDestroyedSound.Play(markerRef)
        EndIf
        If ExtractedExplosion != None
            markerRef.PlaceAtMe(ExtractedExplosion)
        EndIf
    EndIf
    If playerRef != None && StaggerSpell != None
        StaggerSpell.Cast(playerRef, playerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0930_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_PressureGauge_XMarker.GetReference()
    ObjectReference componentRef = Alias_PressureGauge.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_PressureGauge_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_PressureGauge_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_PressureGauge.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullySound != None
        ExtractedSuccessfullySound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0931_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_PressureGauge_XMarker.GetReference()
    ObjectReference componentRef = Alias_PressureGauge.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_PressureGauge_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_PressureGauge_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_PressureGauge.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullySound != None
        ExtractedSuccessfullySound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0932_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_PressureGauge_XMarker.GetReference()
    ObjectReference componentRef = Alias_PressureGauge.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_PressureGauge_Damaged_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_PressureGauge_Damaged_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_PressureGauge.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedDamagedSound != None
        ExtractedDamagedSound.Play(markerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0933_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_PressureGauge_XMarker.GetReference()
    ObjectReference componentRef = Alias_PressureGauge.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_PressureGauge_Destroyed_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_PressureGauge_Destroyed_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_PressureGauge.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None
        If ExtractedDestroyedSound != None
            ExtractedDestroyedSound.Play(markerRef)
        EndIf
        If ExtractedExplosion != None
            markerRef.PlaceAtMe(ExtractedExplosion)
        EndIf
    EndIf
    If playerRef != None && StaggerSpell != None
        StaggerSpell.Cast(playerRef, playerRef)
    EndIf
    If Alias_Components_RefCollection != None && BS01_Invention_ObjectivePercentProgress_Global != None
        BS01_Invention_ObjectivePercentProgress_Global.SetValue(Alias_Components_RefCollection.GetCount() * 100.0 / 3.0)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveDisplayed(1300)
    If BS01_Invention_Valdez_InductionCoil_Scene != None && !BS01_Invention_Valdez_InductionCoil_Scene.IsPlaying()
        BS01_Invention_Valdez_InductionCoil_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1010_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_InductionCoil_XMarker.GetReference()
    ObjectReference componentRef = Alias_InductionCoil.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_InductionCoil_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_InductionCoil_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_InductionCoil.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullySound != None
        ExtractedSuccessfullySound.Play(markerRef)
    EndIf
EndFunction

Function Fragment_Stage_1011_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_InductionCoil_XMarker.GetReference()
    ObjectReference componentRef = Alias_InductionCoil.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_InductionCoil_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_InductionCoil_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_InductionCoil.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedSuccessfullySound != None
        ExtractedSuccessfullySound.Play(markerRef)
    EndIf
EndFunction

Function Fragment_Stage_1012_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_InductionCoil_XMarker.GetReference()
    ObjectReference componentRef = Alias_InductionCoil.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_InductionCoil_Damaged_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_InductionCoil_Damaged_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_InductionCoil.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None && ExtractedDamagedSound != None
        ExtractedDamagedSound.Play(markerRef)
    EndIf
EndFunction

Function Fragment_Stage_1013_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference markerRef = Alias_InductionCoil_XMarker.GetReference()
    ObjectReference componentRef = Alias_InductionCoil.GetReference()
    If playerRef != None && componentRef == None && markerRef != None && BS01_Invention_InductionCoil_Destroyed_MiscItem != None
        componentRef = markerRef.PlaceAtMe(BS01_Invention_InductionCoil_Destroyed_MiscItem, 1, True, False, False)
        If componentRef != None
            Alias_InductionCoil.ForceRefTo(componentRef)
            Alias_Components_RefCollection.AddRef(componentRef)
            playerRef.AddItem(componentRef, 1, True)
        EndIf
    EndIf
    If markerRef != None
        If ExtractedDestroyedSound != None
            ExtractedDestroyedSound.Play(markerRef)
        EndIf
        If ExtractedExplosion != None
            markerRef.PlaceAtMe(ExtractedExplosion)
        EndIf
    EndIf
    If playerRef != None && StaggerSpell != None
        StaggerSpell.Cast(playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1300)
    SetObjectiveDisplayed(1400)
    If BS01_Invention_Valdez_ExtractInductionCoil_Scene != None && !BS01_Invention_Valdez_ExtractInductionCoil_Scene.IsPlaying()
        BS01_Invention_Valdez_ExtractInductionCoil_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    ObjectReference dirtBagRef = Alias_DirtBag_Ref.GetReference()
    If dirtBagRef != None
        If BS01_MQ02_Invention_DirtSackSwap_AV != None
            dirtBagRef.SetValue(BS01_MQ02_Invention_DirtSackSwap_AV, 1.0)
        EndIf
        dirtBagRef.Enable()
        If DirtBagPlacedSound != None
            DirtBagPlacedSound.Play(dirtBagRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference extractionPoint = Alias_UltraciteBatteryExtractionPoint.GetReference()
    ObjectReference batteryRef = Alias_UltraciteBattery.GetReference()
    ObjectReference spawnPoint = Alias_UltraciteFight_SpawnCenterMarker.GetReference()
    RefCollectionAlias robotEnemies = GetAlias(31) as RefCollectionAlias
    ActorBase securityRobotBase = Game.GetFormFromFile(0x005B70FD, "SeventySix.esm") as ActorBase
    SetObjectiveCompleted(1400)
    SetObjectiveDisplayed(1500)
    If playerRef != None && batteryRef == None && extractionPoint != None && BS01_Invention_UltraciteBattery_MiscItem != None
        batteryRef = extractionPoint.PlaceAtMe(BS01_Invention_UltraciteBattery_MiscItem, 1, True, False, False)
        If batteryRef != None
            Alias_UltraciteBattery.ForceRefTo(batteryRef)
            Alias_Components_RefCollection.AddRef(batteryRef)
            playerRef.AddItem(batteryRef, 1, True)
        EndIf
    EndIf
    If extractionPoint != None
        If ExtractedSuccessfullyEjectSound != None
            ExtractedSuccessfullyEjectSound.Play(extractionPoint)
        EndIf
        If ExtractedExplosion != None
            extractionPoint.PlaceAtMe(ExtractedExplosion)
        EndIf
    EndIf
    If playerRef != None && MTR08_CameraShakeSpell != None
        MTR08_CameraShakeSpell.Cast(playerRef, playerRef)
    EndIf
    If BS01_Invention_Valdez_UltraciteFightStart_Scene != None && !BS01_Invention_Valdez_UltraciteFightStart_Scene.IsPlaying()
        BS01_Invention_Valdez_UltraciteFightStart_Scene.Start()
    EndIf
    If spawnPoint == None
        spawnPoint = extractionPoint
    EndIf
    If spawnPoint == None
        spawnPoint = playerRef
    EndIf
    If spawnPoint != None
        spawnPoint.Enable()
        spawnPoint.Activate(playerRef)
    EndIf
    If spawnPoint != None && playerRef != None && robotEnemies != None && robotEnemies.GetCount() == 0 && securityRobotBase != None
        Int spawnIndex = 0
        While spawnIndex < 2
            Actor enemyRef = spawnPoint.PlaceActorAtMe(securityRobotBase, 1)
            If enemyRef != None
                robotEnemies.AddRef(enemyRef)
                enemyRef.StartCombat(playerRef)
            EndIf
            spawnIndex += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(1500)
    SetObjectiveDisplayed(1600)
EndFunction

Function Fragment_Stage_1310_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None
        If BS01_Invention_CoreProcessingUnit_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_CoreProcessingUnit_MiscItem, playerRef.GetItemCount(BS01_Invention_CoreProcessingUnit_MiscItem), True)
        EndIf
        If BS01_Invention_CoreProcessingUnit_Damaged_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_CoreProcessingUnit_Damaged_MiscItem, playerRef.GetItemCount(BS01_Invention_CoreProcessingUnit_Damaged_MiscItem), True)
        EndIf
        If BS01_Invention_CoreProcessingUnit_Destroyed_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_CoreProcessingUnit_Destroyed_MiscItem, playerRef.GetItemCount(BS01_Invention_CoreProcessingUnit_Destroyed_MiscItem), True)
        EndIf
        If BS01_Invention_IonAccelerator_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_IonAccelerator_MiscItem, playerRef.GetItemCount(BS01_Invention_IonAccelerator_MiscItem), True)
        EndIf
        If BS01_Invention_IonAccelerator_Damaged_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_IonAccelerator_Damaged_MiscItem, playerRef.GetItemCount(BS01_Invention_IonAccelerator_Damaged_MiscItem), True)
        EndIf
        If BS01_Invention_IonAccelerator_Destroyed_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_IonAccelerator_Destroyed_MiscItem, playerRef.GetItemCount(BS01_Invention_IonAccelerator_Destroyed_MiscItem), True)
        EndIf
        If BS01_Invention_PressureGauge_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_PressureGauge_MiscItem, playerRef.GetItemCount(BS01_Invention_PressureGauge_MiscItem), True)
        EndIf
        If BS01_Invention_PressureGauge_Damaged_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_PressureGauge_Damaged_MiscItem, playerRef.GetItemCount(BS01_Invention_PressureGauge_Damaged_MiscItem), True)
        EndIf
        If BS01_Invention_PressureGauge_Destroyed_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_PressureGauge_Destroyed_MiscItem, playerRef.GetItemCount(BS01_Invention_PressureGauge_Destroyed_MiscItem), True)
        EndIf
        If BS01_Invention_InductionCoil_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_InductionCoil_MiscItem, playerRef.GetItemCount(BS01_Invention_InductionCoil_MiscItem), True)
        EndIf
        If BS01_Invention_InductionCoil_Damaged_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_InductionCoil_Damaged_MiscItem, playerRef.GetItemCount(BS01_Invention_InductionCoil_Damaged_MiscItem), True)
        EndIf
        If BS01_Invention_InductionCoil_Destroyed_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_InductionCoil_Destroyed_MiscItem, playerRef.GetItemCount(BS01_Invention_InductionCoil_Destroyed_MiscItem), True)
        EndIf
        If BS01_Invention_UltraciteBattery_MiscItem != None
            playerRef.RemoveItem(BS01_Invention_UltraciteBattery_MiscItem, playerRef.GetItemCount(BS01_Invention_UltraciteBattery_MiscItem), True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1315_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_RaiseLarge_Global != None
        playerRef.ModValue(BS01_Invention_ValdezRep_AV, BS01_Invention_ValdezRep_RaiseLarge_Global.GetValue())
    EndIf
    If BS01_MQ02_Invention_ValdezRep_RaiseLarge_Message != None
        BS01_MQ02_Invention_ValdezRep_RaiseLarge_Message.Show()
    EndIf
EndFunction

Function Fragment_Stage_1320_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    Book recommendation = None
    If playerRef != None && BS01_Invention_ValdezRep_AV != None && BS01_Invention_ValdezRep_UpsetThreshold != None
        If playerRef.GetValue(BS01_Invention_ValdezRep_AV) <= BS01_Invention_ValdezRep_UpsetThreshold.GetValue()
            recommendation = BS01_Invention_RecLetter_Bad
        Else
            recommendation = BS01_Invention_RecLetter_Good
        EndIf
    EndIf
    If playerRef != None && recommendation != None
        ObjectReference letterRef = playerRef.PlaceAtMe(recommendation, 1, True, False, False)
        If letterRef != None
            Alias_RecLetter_Ref.ForceRefTo(letterRef)
            playerRef.AddItem(letterRef, 1, True)
        Else
            playerRef.AddItem(recommendation, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    If TryStartFieldTesting()
        If IsStageDone(10000)
            Stop()
        EndIf
    Else
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Bool Function TryStartFieldTesting()
    If IsObjectiveCompleted(1600)
        Return True
    EndIf
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_FieldTesting_QuestStartKeyword != None && BS01_FieldTesting_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        SetObjectiveCompleted(1600)
        Return True
    EndIf
    Return IsObjectiveCompleted(1600)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9000 && IsStageDone(9000)
        If TryStartFieldTesting()
            If IsStageDone(10000)
                Stop()
            EndIf
        Else
            StartTimer(5.0, 9000)
        EndIf
    EndIf
EndEvent

Function Fragment_Stage_10000_Item_00()
    ObjectReference valdezDungeonRef = Alias_Valdez_Dungeon_Ref.GetReference()
    ObjectReference ambientMarker = Alias_AmbientEnemies_EnableMarker.GetReference()
    ObjectReference insectsMarker = Alias_AmbientInsectsEWS_EnableMarker.GetReference()
    If valdezDungeonRef != None
        valdezDungeonRef.Disable()
    EndIf
    If ambientMarker != None
        ambientMarker.Disable()
    EndIf
    If insectsMarker != None
        insectsMarker.Disable()
    EndIf
    If IsStageDone(9000)
        If TryStartFieldTesting()
            Stop()
        Else
            StartTimer(5.0, 9000)
        EndIf
    EndIf
EndFunction
