XPD_Fuel_TrainingScript Function Sim()
    Quest owner = Self as Quest
    Return owner as XPD_Fuel_TrainingScript
EndFunction

Function ThrowAgainstAdam(Int aiPlayerThrow)
    If Sim().ResolveRockPaperScissors(aiPlayerThrow) && ConfettiExplosion != None
        Actor player = GetSimulationPlayer()
        If player != None
            player.PlaceAtMe(ConfettiExplosion, 1, False, False, False)
        EndIf
    EndIf
EndFunction

Actor Function GetSimulationPlayer()
    Actor player = None
    If myPlayer != None
        player = myPlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
    EndIf
    Return player
EndFunction

; Hands the player the "Simulation Results" note for the ending they reached,
; parks it in the Results alias so the bound cleanup script can reclaim it, and
; closes out the simulation exactly once.
Function GiveResults(Form akResults, Bool abSurvived)
    If IsStageDone(4000)
        Return
    EndIf

    Actor player = GetSimulationPlayer()
    If player != None && akResults != None
        ObjectReference note = player.PlaceAtMe(akResults, 1, False, False, False)
        If note != None
            player.AddItem(note, 1, True)
            If Results != None
                Results.ForceRefTo(note)
            EndIf
        Else
            player.AddItem(akResults, 1, True)
        EndIf
        Sim().RememberResults(akResults)
    EndIf

    If abSurvived
        Sim().MarkProgress(6000)
    Else
        Sim().MarkProgress(5000)
    EndIf
    If IsStageDone(121)
        Sim().MarkProgress(350)
    EndIf
    Sim().MarkProgress(4000)
EndFunction

Function Fragment_Stage_0100_Item_00()
    Sim().StartDaily()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    Sim().PickStory()
EndFunction

Function Fragment_Stage_0115_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0120_Item_00()
    Sim().EnterStoryDivider(200)
EndFunction

Function Fragment_Stage_0247_Item_00()
    Sim().MarkProgress(245)
EndFunction

Function Fragment_Stage_0248_Item_00()
    Sim().MarkProgress(245)
EndFunction

Function Fragment_Stage_0250_Item_00()
    If GunMessage != None && IsStageDone(246)
        GunMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0255_Item_00()
    Sim().RollVacuum()
EndFunction

Function Fragment_Stage_0260_Item_00()
    GiveResults(BreathEnding, False)
EndFunction

Function Fragment_Stage_0261_Item_00()
    GiveResults(BurnEnding, True)
EndFunction

Function Fragment_Stage_0262_Item_00()
    GiveResults(CounselEnding, True)
EndFunction

Function Fragment_Stage_0263_Item_00()
    GiveResults(EscapeEnding, True)
EndFunction

Function Fragment_Stage_0264_Item_00()
    GiveResults(ImOutEnding, True)
EndFunction

Function Fragment_Stage_0265_Item_00()
    GiveResults(RoommateEnding, True)
EndFunction

Function Fragment_Stage_0266_Item_00()
    GiveResults(SleepEnding, False)
EndFunction

Function Fragment_Stage_0267_Item_00()
    GiveResults(WaterEnding, True)
EndFunction

Function Fragment_Stage_0268_Item_00()
    GiveResults(VacuumSuccessEnding, True)
EndFunction

Function Fragment_Stage_0269_Item_00()
    GiveResults(VacuumFailEnding, False)
EndFunction

Function Fragment_Stage_0300_Item_00()
    Sim().PickAdamThrow()
EndFunction

Function Fragment_Stage_0305_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AAVeganEnding, True)
EndFunction

Function Fragment_Stage_0319_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AAPoisonEnding, True)
EndFunction

Function Fragment_Stage_0326_Item_00()
    ThrowAgainstAdam(1)
EndFunction

Function Fragment_Stage_0327_Item_00()
    ThrowAgainstAdam(2)
EndFunction

Function Fragment_Stage_0328_Item_00()
    ThrowAgainstAdam(3)
EndFunction

Function Fragment_Stage_0340_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AARubberBandGoodEnding, True)
EndFunction

Function Fragment_Stage_0341_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AARubberBandBadEnding, False)
EndFunction

Function Fragment_Stage_0342_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AADamSabotageGoodEnding, True)
EndFunction

Function Fragment_Stage_0343_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AADamSabotageBadEnding, False)
EndFunction

Function Fragment_Stage_0344_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AAPlayerEscapeGoodEnding, True)
EndFunction

Function Fragment_Stage_0345_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AAPlayerEscapeBadEnding, False)
EndFunction

Function Fragment_Stage_0346_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AAFlowerBadEnding, False)
EndFunction

Function Fragment_Stage_0347_Item_00()
    GiveResults(XPD_Hub_TrainingDay_AAFlowerGoodEnding, True)
EndFunction

Function Fragment_Stage_0414_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDEarlyKillEnding, True)
EndFunction

Function Fragment_Stage_0419_Item_00()
    Sim().MarkProgress(429)
EndFunction

Function Fragment_Stage_0421_Item_00()
    Sim().MarkProgress(429)
EndFunction

Function Fragment_Stage_0423_Item_00()
    Sim().MarkProgress(429)
EndFunction

Function Fragment_Stage_0425_Item_00()
    Sim().RollAmputation(AmputateChanceIdealNum)
EndFunction

Function Fragment_Stage_0426_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDEarlyKillEnding, True)
EndFunction

Function Fragment_Stage_0428_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDAmputationEnding, True)
EndFunction

Function Fragment_Stage_0432_Item_00()
    Sim().RollDiagnosis()
EndFunction

Function Fragment_Stage_0433_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDMedicationGoodEnding, True)
EndFunction

Function Fragment_Stage_0434_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDMedicationBadEnding, True)
EndFunction

Function Fragment_Stage_0435_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDPlaceboGoodEnding, True)
EndFunction

Function Fragment_Stage_0436_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDPlaceboBadEnding, True)
EndFunction

Function Fragment_Stage_0437_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDAntibioticsGoodEnding, True)
EndFunction

Function Fragment_Stage_0438_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDAntibioticsBadEnding, True)
EndFunction

Function Fragment_Stage_0439_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDStageDeathGoodEnding, True)
EndFunction

Function Fragment_Stage_0440_Item_00()
    GiveResults(XPD_Hub_TrainingDay_TGDStageDeathBadEnding, True)
EndFunction

Function Fragment_Stage_0441_Item_00()
    Sim().ChooseDiagnosis()
EndFunction

Function Fragment_Stage_0442_Item_00()
    Sim().ChooseDiagnosis()
EndFunction

Function Fragment_Stage_0443_Item_00()
    Sim().ChooseDiagnosis()
EndFunction

Function Fragment_Stage_0444_Item_00()
    Sim().ChooseDiagnosis()
EndFunction

Function Fragment_Stage_0514_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDVaultEscapeEnding, True)
EndFunction

Function Fragment_Stage_0519_Item_00()
    Sim().MarkProgress(531)
EndFunction

Function Fragment_Stage_0521_Item_00()
    Sim().MarkProgress(531)
EndFunction

Function Fragment_Stage_0523_Item_00()
    Sim().MarkProgress(531)
EndFunction

Function Fragment_Stage_0525_Item_00()
    Sim().RollReactorFix(ReactorFixIdealNum)
EndFunction

Function Fragment_Stage_0526_Item_00()
    Sim().RollReactorFix(ReactorFixIdealNum)
EndFunction

Function Fragment_Stage_0528_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDFixedReactorEnding, True)
EndFunction

Function Fragment_Stage_0532_Item_00()
    Sim().RollReactorKnob(ReactorFixIdealNum)
EndFunction

Function Fragment_Stage_0533_Item_00()
    Sim().RollReactorKnob(ReactorFixIdealNum)
EndFunction

Function Fragment_Stage_0536_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDFixReactorGoodEnding, True)
EndFunction

Function Fragment_Stage_0537_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDFixReactorBadEnding, False)
EndFunction

Function Fragment_Stage_0538_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDManualFixGoodEnding, True)
EndFunction

Function Fragment_Stage_0539_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDManualFixBadEnding, False)
EndFunction

Function Fragment_Stage_0540_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDGumGoodEnding, True)
EndFunction

Function Fragment_Stage_0541_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDGumBadEnding, False)
EndFunction

Function Fragment_Stage_0542_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDSacrificeGoodEnding, False)
EndFunction

Function Fragment_Stage_0543_Item_00()
    GiveResults(XPD_Hub_TrainingDay_MDSacrificeBadEnding, False)
EndFunction

Function Fragment_Stage_0608_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCKillPasserbyEnding, True)
EndFunction

Function Fragment_Stage_0613_Item_00()
    Sim().MarkProgress(620)
EndFunction

Function Fragment_Stage_0614_Item_00()
    Sim().MarkProgress(621)
EndFunction

Function Fragment_Stage_0615_Item_00()
    Sim().MarkProgress(620)
EndFunction

Function Fragment_Stage_0616_Item_00()
    Sim().MarkProgress(620)
EndFunction

Function Fragment_Stage_0618_Item_00()
    Sim().MarkProgress(620)
EndFunction

Function Fragment_Stage_0619_Item_00()
    Sim().MarkProgress(621)
EndFunction

Function Fragment_Stage_0622_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCSockPuppetEnding, True)
EndFunction

Function Fragment_Stage_0623_Item_00()
    Sim().RollThiefCall()
EndFunction

Function Fragment_Stage_0624_Item_00()
    Sim().RollThiefCall()
EndFunction

Function Fragment_Stage_0627_Item_00()
    Sim().MarkProgress(621)
EndFunction

Function Fragment_Stage_0628_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCLootSuccessEnding, True)
EndFunction

Function Fragment_Stage_0629_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCLootFailEnding, True)
EndFunction

Function Fragment_Stage_0630_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCDistractionGoodEnding, True)
EndFunction

Function Fragment_Stage_0631_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCDistractionBadEnding, True)
EndFunction

Function Fragment_Stage_0632_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCShootSuccesstEnding, True)
EndFunction

Function Fragment_Stage_0633_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCShootFailEnding, True)
EndFunction

Function Fragment_Stage_0634_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCLeaveGoodEnding, True)
EndFunction

Function Fragment_Stage_0635_Item_00()
    GiveResults(XPD_Hub_TrainingDay_SCLeaveBadEnding, True)
EndFunction

Function Fragment_Stage_4000_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_5000_Item_00()
EndFunction

Function Fragment_Stage_6000_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
    Sim().CompleteDaily()
EndFunction
