Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    RecordCheckpoint(100)
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    RecordCheckpoint(300)
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveDisplayed(310, True)
EndFunction

Function Fragment_Stage_0315_Item_00()
    SetObjectiveCompleted(310, True)
    If SFM04_Organic_Radio != None && !SFM04_Organic_Radio.IsRunning()
        SFM04_Organic_Radio.Start()
    EndIf
    If !IsStageDone(340) && !IsStageDone(320)
        SetStage(320)
    EndIf
    ReconcileTrackerRadio()
EndFunction

; FO76 filled the tracker's TargetMarker (alias 2) from the start event's
; Reference1. The converted station starts with the game instead, so aim its
; distance-banded beeps at the bone meal nest once the tracker is installed.
Function ReconcileTrackerRadio()
    If SFM04_Organic_Radio == None || !SFM04_Organic_Radio.IsRunning() || !IsStageDone(315) || IsStageDone(340)
        Return
    EndIf
    ReferenceAlias trackerTarget = SFM04_Organic_Radio.GetAlias(2) as ReferenceAlias
    ObjectReference nest = None
    If Alias_BoneMealContainer != None
        nest = Alias_BoneMealContainer.GetReference()
    EndIf
    If trackerTarget == None || nest == None
        Return
    EndIf
    If trackerTarget.GetReference() != nest
        trackerTarget.ForceRefTo(nest)
    EndIf
    ; Tuning in before installation ends the scene once no distance band applies.
    Scene trackerScene = Game.GetFormFromFile(0x001D1410, "SeventySix.esm") as Scene
    If trackerScene != None && !trackerScene.IsPlaying()
        trackerScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveDisplayed(320, True)
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveCompleted(320, True)
EndFunction

Function Fragment_Stage_0340_Item_00()
    RecordCheckpoint(340)
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(310, False)
    SetObjectiveDisplayed(320, False)
    If !IsStageDone(341)
        SetStage(341)
    EndIf
    If !IsStageDone(380)
        SetStage(380)
    EndIf
    ; The tracker's stop stage is set by this bone meal milestone.
    If SFM04_Organic_Radio != None && SFM04_Organic_Radio.IsRunning() && !SFM04_Organic_Radio.IsStageDone(1000)
        SFM04_Organic_Radio.SetStage(1000)
    EndIf
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0380_Item_00()
    If !IsStageDone(350)
        SetObjectiveDisplayed(350, True)
    EndIf
    If !IsStageDone(360)
        SetObjectiveDisplayed(360, True)
    EndIf
    If !IsStageDone(370)
        SetObjectiveDisplayed(370, True)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(350, True)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0360_Item_00()
    SetObjectiveCompleted(360, True)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0370_Item_00()
    SetObjectiveCompleted(370, True)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0400_Item_00()
    RecordCheckpoint(400)
    If !IsStageDone(401)
        SetStage(401)
    EndIf
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
    If IsStageDone(420) && IsStageDone(430)
        SetStage(435)
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    If IsStageDone(410) && IsStageDone(430)
        SetStage(435)
    EndIf
EndFunction

Function Fragment_Stage_0430_Item_00()
    If IsStageDone(410) && IsStageDone(420)
        SetStage(435)
    EndIf
EndFunction

Function Fragment_Stage_0435_Item_00()
    If IsStageDone(440)
        SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_0440_Item_00()
    If IsStageDone(435)
        SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    ReconcileChemistryGlobals()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(475, True)
EndFunction

Function Fragment_Stage_0460_Item_00()
    ReconcileChemistryGlobals()
    SetEnableMarker(Alias_DyerMixingSoundEnable, !IsStageDone(600))
    SetObjectiveCompleted(475, True)
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function ReconcileChemistryGlobals()
    If IsStageDone(450) && SFM04_Organic_AllIngredientsPlacedGlobal != None
        SFM04_Organic_AllIngredientsPlacedGlobal.SetValue(1.0)
    EndIf
    If IsStageDone(460) && SFM04_Organic_AllIngredientsMixedGlobal != None
        SFM04_Organic_AllIngredientsMixedGlobal.SetValue(1.0)
    EndIf
    If IsStageDone(460) && !IsStageDone(500)
        SetStage(500)
    EndIf
    If IsStageDone(600) && !IsStageDone(700) && Alias_StranglerBloom != None && Alias_StranglerBloom.GetReference() != None
        Alias_StranglerBloom.GetReference().EnableNoWait()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    RecordCheckpoint(600)
    SetEnableMarker(Alias_DyerMixingSoundEnable, False)
    SetEnableMarker(Alias_DyerFlushingSoundEnable, True)
    SetObjectiveCompleted(500, True)
    SetObjectiveDisplayed(600, True)
    If Alias_StranglerBloom != None && Alias_StranglerBloom.GetReference() != None
        Alias_StranglerBloom.GetReference().Enable()
    EndIf
    If SFM04_Organic_Blooms != None && !SFM04_Organic_Blooms.IsRunning() && SFM04_Organic_Blooms_StartKeyword != None
        SFM04_Organic_Blooms_StartKeyword.SendStoryEventAndWait(None, Game.GetPlayer(), Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600, True)
    SetObjectiveDisplayed(700, True)
    SFM04_Organic_PlayerAliasScript playerScript = Alias_SFM04Player as SFM04_Organic_PlayerAliasScript
    If playerScript != None
        playerScript.ReconcileRadShield()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(700, True)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && SFM04_Cultivate_Completed != None
        playerRef.SetValue(SFM04_Cultivate_Completed, 1.0)
    EndIf
    If SFM04_Organic_Radio != None && SFM04_Organic_Radio.IsRunning()
        SFM04_Organic_Radio.Stop()
    EndIf
    SetEnableMarker(Alias_DyerMixingSoundEnable, False)
    SetEnableMarker(Alias_DyerFlushingSoundEnable, False)
    Stop()
EndFunction

; The initially disabled Dyer Chemical markers parent the vat mixing loop and the
; flush vent loop. Existing saves get the state their stages imply.
Function ReconcileDyerSounds()
    If IsStageDone(1000)
        Return
    EndIf
    SetEnableMarker(Alias_DyerMixingSoundEnable, IsStageDone(460) && !IsStageDone(600))
    SetEnableMarker(Alias_DyerFlushingSoundEnable, IsStageDone(600))
EndFunction

Function SetEnableMarker(ReferenceAlias akMarker, Bool abEnabled)
    ObjectReference marker = None
    If akMarker != None
        marker = akMarker.GetReference()
    EndIf
    If marker == None
        Return
    EndIf
    If abEnabled && marker.IsDisabled()
        marker.EnableNoWait()
    ElseIf !abEnabled && !marker.IsDisabled()
        marker.DisableNoWait()
    EndIf
EndFunction
Function TryAdvanceToChemicalDeposit()
    If IsStageDone(340) && IsStageDone(350) && IsStageDone(360) && IsStageDone(370) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction
Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && SFM04_Organic_StartedValue != None
        playerRef.SetValue(SFM04_Organic_StartedValue, 1.0)
    EndIf
    If !IsStageDone(100) && !IsStageDone(200) && !IsStageDone(300)
        SetStage(50)
    EndIf
EndFunction

Function RecordCheckpoint(Int aiCheckpoint)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && SFM04_CheckpointValue != None && playerRef.GetValue(SFM04_CheckpointValue) < aiCheckpoint
        playerRef.SetValue(SFM04_CheckpointValue, aiCheckpoint)
    EndIf
EndFunction
