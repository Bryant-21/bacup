; Stages 300-1000 mean a marcher was found, which opens that marcher's task objective (30-100); stages 2100-2800 close those tasks.
; Stages 1-6 are debug boss switches handled by E01F_FasnachtQuest.
Function StartSceneIfStopped(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

; The bonfire, effigy, chosen decorations and stein report stay up after a run ends; the next run clears them.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    If Bonfire != None
        E01F_Fasnacht_Bonfire bonfireRef = Bonfire.GetReference() as E01F_Fasnacht_Bonfire
        If bonfireRef != None
            bonfireRef.TurnOffBonfire()
        EndIf
    EndIf
    DisableAliasRef(Bonfire)
    DisableAliasRef(OldManWinter)
    DisableAliasRef(FinalReport)
    If Alias_AllDecorMarkers != None
        Alias_AllDecorMarkers.DisableAll()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(90, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(20, True)
EndFunction

; Helping a marcher (2100-2800) starts its ToParadeStart scene, whose completion sets its 3100-3800 arrival stage;
; E01F_FasnachtQuest sets 4000 once every selected marcher has arrived.
Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(30, True)
    StartSceneIfStopped(BakerParadeStart)
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveCompleted(40, True)
    StartSceneIfStopped(BeekeeperParadeStart)
EndFunction

Function Fragment_Stage_2300_Item_00()
    SetObjectiveCompleted(50, True)
    StartSceneIfStopped(ButcherParadeStart)
EndFunction

Function Fragment_Stage_2400_Item_00()
    SetObjectiveCompleted(60, True)
    StartSceneIfStopped(CandlemakerParadeStart)
EndFunction

Function Fragment_Stage_2500_Item_00()
    SetObjectiveCompleted(70, True)
    StartSceneIfStopped(DecoratorParadeStart)
EndFunction

Function Fragment_Stage_2600_Item_00()
    SetObjectiveCompleted(80, True)
    StartSceneIfStopped(LibrarianParadeStart)
EndFunction

Function Fragment_Stage_2700_Item_00()
    SetObjectiveCompleted(90, True)
    StartSceneIfStopped(MusicianParadeStart)
EndFunction

Function Fragment_Stage_2800_Item_00()
    SetObjectiveCompleted(100, True)
    StartSceneIfStopped(WoodsmanParadeStart)
EndFunction

Function Fragment_Stage_2900_Item_00()
    SetObjectiveDisplayed(110, True)
EndFunction

Function MarcherArrived()
    E01F_FasnachtQuest fasnacht = (Self as Quest) as E01F_FasnachtQuest
    If fasnacht != None
        fasnacht.MarcherArrived()
    EndIf
EndFunction

Function Fragment_Stage_3100_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3200_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3300_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3400_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3500_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3600_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3700_Item_00()
    MarcherArrived()
EndFunction

Function Fragment_Stage_3800_Item_00()
    MarcherArrived()
EndFunction

; MCParadeStart's completion sets 4050.
Function Fragment_Stage_4000_Item_00()
    StartSceneIfStopped(MCParadeStart)
EndFunction

Function Fragment_Stage_4050_Item_00()
    SetStage(4100)
EndFunction

; Stage 5000/6000/7000 arm the MC's distance checks against the three attack markers (5100/6100/7100).
Function Fragment_Stage_4100_Item_00()
    E01F_FasnachtQuest fasnacht = (Self as Quest) as E01F_FasnachtQuest
    If fasnacht != None
        fasnacht.FillParadeMarchers()
    EndIf
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(120, True)
    SetObjectiveDisplayed(140, True)
    SetObjectiveDisplayed(160, True)
    StartSceneIfStopped(ParadeStart)
    SetStage(5000)
EndFunction

Function Fragment_Stage_5200_Item_00()
    Wave1Message.Show()
EndFunction

Function Fragment_Stage_5300_Item_00()
    SetStage(6000)
EndFunction

Function Fragment_Stage_6200_Item_00()
    Wave2Message.Show()
EndFunction

Function Fragment_Stage_6300_Item_00()
    SetStage(7000)
EndFunction

Function Fragment_Stage_7200_Item_00()
    Wave3Message.Show()
EndFunction

; ParadeSuccess walks the MC to the bonfire; its completion sets 8000.
Function Fragment_Stage_7400_Item_00()
    StartSceneIfStopped(ParadeSuccess)
EndFunction

; The bonfire and Old Man Winter are placed initially disabled.
Function Fragment_Stage_8000_Item_00()
    SetObjectiveCompleted(120, True)
    SetObjectiveCompleted(140, True)
    SetObjectiveCompleted(160, True)
    SetObjectiveDisplayed(130, True)
    EnableAliasRef(OldManWinter)
    EnableAliasRef(Bonfire)
EndFunction

Function EnableAliasRef(ReferenceAlias akAlias)
    If akAlias != None && akAlias.GetReference() != None
        akAlias.GetReference().Enable()
    EndIf
EndFunction

Function DisableAliasRef(ReferenceAlias akAlias)
    If akAlias != None && akAlias.GetReference() != None
        akAlias.GetReference().Disable()
    EndIf
EndFunction

; Each ParadeMarcher death stage (8010-8050) is one lost marcher.
Function Fragment_Stage_8100_Item_00()
    Int survivors = 5
    Int deathStage = 8010
    While deathStage <= 8050
        If IsStageDone(deathStage)
            survivors -= 1
        EndIf
        deathStage += 10
    EndWhile
    If SurvivingMarchers != None
        SurvivingMarchers.SetValue(survivors)
    EndIf
    SetObjectiveCompleted(130, True)
    If Bonfire != None
        E01F_Fasnacht_Bonfire bonfireRef = Bonfire.GetReference() as E01F_Fasnacht_Bonfire
        If bonfireRef != None
            bonfireRef.GoToState("bonfirelit")
        EndIf
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    SetObjectiveFailed(10, True)
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    SetObjectiveFailed(20, True)
    Stop()
EndFunction

Function Fragment_Stage_9993_Item_00()
    SetObjectiveFailed(120, True)
    SetObjectiveFailed(140, True)
    SetObjectiveFailed(160, True)
    Stop()
EndFunction

Function Fragment_Stage_9994_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
