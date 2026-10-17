; Quests:E07B_Invaders:QuestScript owns waves, kill counts, shields, pulses and missile strikes; these
; fragments own objectives, messages and boss spawns. Activating Siphon Kappa sets 200.
Quests:E07B_Invaders:QuestScript Function Controller()
    Return (Self as Quest) as Quests:E07B_Invaders:QuestScript
EndFunction

Function SpawnBoss(ActorBase akBase, ReferenceAlias akSpawnPoint, ReferenceAlias akBossAlias)
    If akBase == None || akSpawnPoint == None || akSpawnPoint.GetReference() == None || akBossAlias == None
        Return
    EndIf
    Actor boss = akSpawnPoint.GetReference().PlaceActorAtMe(akBase)
    If boss != None
        akBossAlias.ForceRefTo(boss)
    EndIf
EndFunction

Function ShowMessage(Message akMessage)
    If akMessage != None
        akMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If Alias_RefsToEnable != None
        Alias_RefsToEnable.EnableAll()
    EndIf
    If Alias_RefsToDisable != None
        Alias_RefsToDisable.DisableAll()
    EndIf
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.StartEvent()
    EndIf
    SetObjectiveDisplayed(15, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    ShowMessage(E07B_Invaders_AliensAlertedMessage)
    SetObjectiveDisplayed(20, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.BeginPhase(1)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(20, True)
    SpawnBoss(E07B_Invaders_ZetanAlien_Kappa_MiniBoss01, Alias_Furn_BossSpawnKappa, Alias_Actor_MiniBoss01)
    SetObjectiveDisplayed(22, True)
EndFunction

Function Fragment_Stage_0260_Item_00()
    SetObjectiveCompleted(22, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.DropShield(Alias_Activator_CenterProbe)
    EndIf
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0270_Item_00()
    SetObjectiveCompleted(30, True)
    ShowMessage(E07B_Invaders_Message_StartingOmega)
    SetObjectiveDisplayed(31, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.WatchArrival(19)
    EndIf
EndFunction

Function Fragment_Stage_0280_Item_00()
    SetObjectiveCompleted(31, True)
    SetObjectiveDisplayed(32, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.BeginPhase(2)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(32, True)
    ShowMessage(E07B_Invaders_CosmicColonelMessage)
    SpawnBoss(E07B_Invaders_AlienZetanDrone_MiniBoss02_Alpha, Alias_Furn_BossSpawnAlpha, Alias_Actor_MiniBoss02)
    SpawnBoss(E07B_Invaders_AlienZetanDrone_MiniBoss02_Omega, Alias_Furn_BossSpawnOmega, Alias_Actor_MiniBoss02_Omega)
    SetObjectiveDisplayed(33, True)
    SetObjectiveDisplayed(34, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(33, True)
    If IsStageDone(312)
        SetStage(314)
    EndIf
EndFunction

Function Fragment_Stage_0312_Item_00()
    SetObjectiveCompleted(34, True)
    If IsStageDone(310)
        SetStage(314)
    EndIf
EndFunction

Function Fragment_Stage_0314_Item_00()
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.DropShield(Alias_Activator_ExtractorOmega)
    EndIf
    SetObjectiveDisplayed(36, True)
EndFunction

Function Fragment_Stage_0315_Item_00()
    SetObjectiveCompleted(36, True)
    ShowMessage(E07B_Invaders_Message_StartingZeta)
    SetObjectiveDisplayed(38, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.WatchArrival(20)
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(38, True)
    SetObjectiveDisplayed(40, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.BeginPhase(3)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40, True)
    ShowMessage(E07B_Invaders_GalacticGeneralMessage)
    SpawnBoss(E07B_Invaders_AlienZetanInvader_Boss, Alias_Furn_BossSpawnZeta, Alias_Actor_Boss)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50, True)
    Quests:E07B_Invaders:QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.DropShield(Alias_Activator_ExtractorZeta)
    EndIf
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9992_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    If Alias_RefsToEnable != None
        Alias_RefsToEnable.DisableAll()
    EndIf
    If Alias_RefsToDisable != None
        Alias_RefsToDisable.EnableAll()
    EndIf
EndFunction
