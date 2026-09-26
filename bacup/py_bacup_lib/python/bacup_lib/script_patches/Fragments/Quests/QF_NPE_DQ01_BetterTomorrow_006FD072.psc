Function Fragment_Stage_0100_Item_00()
    PublishTaskTotals()
    RegisterForLane()
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0175_Item_00()
    RegisterForLane()
    If !IsStageDone(10) && !IsStageDone(20) && !IsStageDone(30)
        SetStage(ChooseDailyTaskStage())
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    If !IsStageDone(10) && !IsStageDone(20) && !IsStageDone(30)
        SetStage(ChooseDailyTaskStage())
    EndIf
    If IsStageDone(10)
        SetStage(300)
    ElseIf IsStageDone(20)
        SetStage(400)
    ElseIf IsStageDone(30)
        SetStage(500)
    Else
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    MarkTaskStarted(NPE_DQ01_DidCarePkgTask_AV)
    PublishTaskTotals()
    ObjectReference bandageRef = Alias_MiscItem_BandageToEnable.GetReference()
    ObjectReference foodParcelRef = Alias_MiscItem_FoodParcelToEnable.GetReference()
    ObjectReference missiveRef = Alias_MiscItem_MissiveToEnable.GetReference()
    If bandageRef != None
        bandageRef.Enable()
    EndIf
    If foodParcelRef != None
        foodParcelRef.Enable()
    EndIf
    If missiveRef != None
        missiveRef.Enable()
    EndIf
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30, True, True)
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40, True, True)
EndFunction

Function Fragment_Stage_0330_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        Int carePackageCount = playerRef.GetItemCount(NPE_DQ01_CarePackage)
        If carePackageCount > CarePkgTotal
            carePackageCount = CarePkgTotal
        EndIf
        If carePackageCount > 0
            playerRef.RemoveItem(NPE_DQ01_CarePackage, carePackageCount, True)
        EndIf
    EndIf
    SetObjectiveCompleted(40)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    MarkTaskStarted(NPE_DQ01_DidTrapHarvestTask_AV)
    SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(51, True, True)
EndFunction

; The second trap is the ambush: alias 0 carries DefaultAliasOnDistanceLessThan with
; StageToSet 415, and the quest's own encounter-wave rows hold the "Wolves" wave.
Function Fragment_Stage_0415_Item_00()
    Quest owner = Self as Quest
    If owner as DefaultQuestEncounterWaveScript
        (owner as DefaultQuestEncounterWaveScript).StartEncounterWaveByID("Wolves")
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetObjectiveCompleted(51)
    SetObjectiveDisplayed(52, True, True)
EndFunction

Function Fragment_Stage_0430_Item_00()
    SetObjectiveCompleted(52)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    MarkTaskStarted(NPE_DQ01_DidCultResearchTask_AV)
    PublishTaskTotals()
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_0505_Item_00()
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(60)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Quest owner = Self as Quest
    If owner as DefaultQuestEncounterWaveScript
        (owner as DefaultQuestEncounterWaveScript).StopEncounterWaveByID("Wolves", True)
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        If IsStageDone(430)
            Int meatCount = playerRef.GetItemCount(NPE_DQ01_FreshMeat)
            If meatCount > 3
                meatCount = 3
            EndIf
            If meatCount > 0
                playerRef.RemoveItem(NPE_DQ01_FreshMeat, meatCount, True)
            EndIf
        ElseIf IsStageDone(510)
            Int artifactCount = playerRef.GetItemCount(NPE_DQ01_CultistArtifact)
            If artifactCount > ArtifactsTotal
                artifactCount = ArtifactsTotal
            EndIf
            If artifactCount > 0
                playerRef.RemoveItem(NPE_DQ01_CultistArtifact, artifactCount, True)
            EndIf
        EndIf
    EndIf
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(70)
    SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Quest owner = Self as Quest
    If owner as DefaultQuestEncounterWaveScript
        (owner as DefaultQuestEncounterWaveScript).StopAllEncounterWaves(True)
    EndIf
    Stop()
EndFunction

; Objectives 20 and 60 read "(<Global=...Count>/<Global=...Total>)". The shared inventory
; rows publish the counts through ItemCountTextVar, but no binding on this quest sets
; ItemRequiredAmountTextVar, so the totals would read 0. Republished on every run because
; the quest resets each game day.
Function PublishTaskTotals()
    Int carePackageTotal = CarePkgTotal
    If carePackageTotal <= 0
        carePackageTotal = 3
    EndIf
    Int artifactTotal = ArtifactsTotal
    If artifactTotal <= 0
        artifactTotal = 4
    EndIf

    Quest owner = Self as Quest
    If owner as B21:QuestVariables
        (owner as B21:QuestVariables).SetVariable("CarePkgTotal", carePackageTotal as Float)
        (owner as B21:QuestVariables).SetVariable("ArtifactsTotal", artifactTotal as Float)
    EndIf
EndFunction

; FO76 picked today's errand in Lane's dialogue, which the conversion cannot run. Rotate
; through the three tasks using the per-player "did this task" actor values the quest
; already carries, so the next day's run is not the same errand again.
Int Function ChooseDailyTaskStage()
    Actor player = Game.GetPlayer()
    Bool didCarePkg = HasDoneTask(player, NPE_DQ01_DidCarePkgTask_AV)
    Bool didTrapHarvest = HasDoneTask(player, NPE_DQ01_DidTrapHarvestTask_AV)
    Bool didCultResearch = HasDoneTask(player, NPE_DQ01_DidCultResearchTask_AV)
    If didCarePkg && didTrapHarvest && didCultResearch
        ClearTaskHistory(player)
        didCarePkg = False
        didTrapHarvest = False
        didCultResearch = False
    EndIf

    Int roll = Utility.RandomInt(0, 2)
    Int attempts = 0
    While attempts < 3
        Int candidate = (roll + attempts) % 3
        If candidate == 0 && !didCarePkg
            Return 10
        ElseIf candidate == 1 && !didTrapHarvest
            Return 20
        ElseIf candidate == 2 && !didCultResearch
            Return 30
        EndIf
        attempts += 1
    EndWhile
    Return 20
EndFunction

Bool Function HasDoneTask(Actor akPlayer, ActorValue akTaskValue)
    Return akPlayer != None && akTaskValue != None && akPlayer.GetValue(akTaskValue) >= 1.0
EndFunction

Function MarkTaskStarted(ActorValue akTaskValue)
    Actor player = Game.GetPlayer()
    If player != None && akTaskValue != None
        player.SetValue(akTaskValue, 1.0)
    EndIf
EndFunction

Function ClearTaskHistory(Actor akPlayer)
    If akPlayer == None
        Return
    EndIf
    If NPE_DQ01_DidCarePkgTask_AV != None
        akPlayer.SetValue(NPE_DQ01_DidCarePkgTask_AV, 0.0)
    EndIf
    If NPE_DQ01_DidTrapHarvestTask_AV != None
        akPlayer.SetValue(NPE_DQ01_DidTrapHarvestTask_AV, 0.0)
    EndIf
    If NPE_DQ01_DidCultResearchTask_AV != None
        akPlayer.SetValue(NPE_DQ01_DidCultResearchTask_AV, 0.0)
    EndIf
EndFunction

; Lane's converted dialogue never sets stage 200 (accept the errand) or stage 700 (hand it
; in), so both ends of the quest stall on their objectives. Substitute talking to him with
; activating him and waiting for the conversation to finish.
Function RegisterForLane()
    ReferenceAlias laneAlias = GetAlias(14) as ReferenceAlias
    If laneAlias != None && laneAlias.GetReference() != None
        RegisterForRemoteEvent(laneAlias.GetReference(), "OnActivate")
    EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
    Actor player = Game.GetPlayer()
    ReferenceAlias laneAlias = GetAlias(14) as ReferenceAlias
    If player == None || akActivator != player || laneAlias == None || akSender != laneAlias.GetReference()
        Return
    EndIf

    Int nextStage = -1
    If IsStageDone(600) && !IsStageDone(700)
        nextStage = 700
    ElseIf !IsStageDone(200)
        nextStage = 200
    EndIf
    WaitForDialogueAndSetStage(akSender as Actor, player, nextStage)
EndEvent

Function WaitForDialogueAndSetStage(Actor akSpeaker, Actor akPlayer, Int aiStage)
    If akSpeaker == None || akPlayer == None || aiStage < 0
        Return
    EndIf
    Int checks = 0
    While akSpeaker.GetDialogueTarget() != akPlayer && checks < 40
        Utility.Wait(0.25)
        checks += 1
    EndWhile
    If akSpeaker.GetDialogueTarget() != akPlayer
        Return
    EndIf
    checks = 0
    While akSpeaker.GetDialogueTarget() == akPlayer && checks < 2400
        Utility.Wait(0.25)
        checks += 1
    EndWhile
    If akSpeaker.GetDialogueTarget() != akPlayer && !IsStageDone(aiStage)
        SetStage(aiStage)
    EndIf
EndFunction
