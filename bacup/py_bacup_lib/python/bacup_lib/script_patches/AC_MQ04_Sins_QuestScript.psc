Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript Function BossController()
    Return Alias_JerseyDevil as Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript
EndFunction

Function RebuildPheromoneCounter()
    pheromoneCounter = 0
    If IsStageDone(150)
        pheromoneCounter += 1
    EndIf
    If IsStageDone(152)
        pheromoneCounter += 1
    EndIf
    If IsStageDone(154)
        pheromoneCounter += 1
    EndIf
    If IsStageDone(156)
        pheromoneCounter += 1
    EndIf
    If IsStageDone(158)
        pheromoneCounter += 1
    EndIf
EndFunction

Function RecordPheromonePlaced(Int stageId)
    If stageId < pheromonesStageMin || stageId > pheromonesStageMax
        Return
    EndIf
    RebuildPheromoneCounter()
    If pheromoneCounter >= pheromoneTotal && !IsStageDone(pheromonesPlacedStage)
        SetStage(pheromonesPlacedStage)
    EndIf
EndFunction

Function BeginJerseyDevilFight()
    Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript controller = BossController()
    If controller != None
        controller.PrepareForFight()
    EndIf
EndFunction

Function DownJerseyDevil()
    Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript controller = BossController()
    If controller != None
        controller.EnterDownedState()
    EndIf
EndFunction

Function ReleaseJerseyDevil()
    Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript controller = BossController()
    If controller != None
        controller.ReleaseAfterHarvest()
    EndIf
EndFunction

Function CleanupQuest()
    Quests:AC_MQ04_SinsOfTheFather:JerseyDevilBossScript controller = BossController()
    If controller != None
        controller.CleanupBoss()
    EndIf
EndFunction

Event OnQuestInit()
    RebuildPheromoneCounter()
EndEvent

Event OnQuestShutdown()
    CleanupQuest()
EndEvent
