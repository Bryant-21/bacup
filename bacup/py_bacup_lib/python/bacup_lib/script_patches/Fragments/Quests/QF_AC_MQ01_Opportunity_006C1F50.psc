Actor Function ActorFromAlias(ReferenceAlias actorAlias)
    If actorAlias == None
        Return None
    EndIf
    Return actorAlias.GetActorReference()
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function RestoreActor(ReferenceAlias actorAlias)
    Actor actorRef = ActorFromAlias(actorAlias)
    If actorRef != None
        actorRef.Enable()
        actorRef.EvaluatePackage()
    EndIf
EndFunction

Function RestoreCoreActors()
    RestoreActor(Alias_Actor_Vin)
    RestoreActor(Alias_Actor_Evelyn)
    RestoreActor(Alias_Actor_Abbie)
EndFunction

Function SetHitmenCombatState(Bool hostile)
    Int index = 0
    While index < Alias_Actors_AllHitmen.GetCount()
        Actor hitman = Alias_Actors_AllHitmen.GetAt(index) as Actor
        If hitman != None
            hitman.Enable()
            hitman.SetGhost(!hostile)
            If hostile
                hitman.RemoveFromFaction(CaptiveFaction)
                hitman.AddToFaction(AC_MQ01_Opportunity_EnemyFaction)
            EndIf
            hitman.EvaluatePackage()
        EndIf
        index += 1
    EndWhile
EndFunction

Function TryFinishHitmanFight()
    If IsStageDone(670) && IsStageDone(680) && IsStageDone(690) && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Quests:AC_MQ01_Opportunity:QuestScript Function QuestController()
    Return (Self as Quest) as Quests:AC_MQ01_Opportunity:QuestScript
EndFunction

Function Fragment_Stage_0010_Item_00()
    RestoreCoreActors()
    If IsStageDone(600) && !IsStageDone(700)
        SetHitmenCombatState(True)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    RestoreCoreActors()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    Quests:AC_MQ01_Opportunity:QuestScript controller = QuestController()
    If controller != None
        controller.BeginPerformanceDucking()
    EndIf
    StartSceneIfStopped(AC_MQ01_Opportunity_EvelynPerformance)
EndFunction

Function Fragment_Stage_0350_Item_00()
    RestoreCoreActors()
    StartSceneIfStopped(AC_MQ01_Opportunity_EvelynPerformance)
EndFunction

Function Fragment_Stage_0360_Item_00()
    Actor abbie = ActorFromAlias(Alias_Actor_Abbie)
    Actor vin = ActorFromAlias(Alias_Actor_Vin)
    If abbie != None
        abbie.PlayIdle(IdleClapping)
    EndIf
    If vin != None
        vin.PlayIdle(IdleClapping)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
    Quests:AC_MQ01_Opportunity:QuestScript controller = QuestController()
    If controller != None
        controller.EndPerformanceDucking()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
    RestoreActor(Alias_Actor_Vin)
EndFunction

Function Fragment_Stage_0460_Item_00()
    RestoreActor(Alias_Actor_Vin)
EndFunction

Function Fragment_Stage_0470_Item_00()
    RestoreActor(Alias_Actor_Vin)
EndFunction

Function Fragment_Stage_0499_Item_00()
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(45)
    RestoreCoreActors()
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(50)
    RestoreActor(Alias_Actor_Musician)
    Quests:AC_MQ01_Opportunity:QuestScript controller = QuestController()
    If controller != None
        controller.BeginPerformanceDucking()
    EndIf
    StartSceneIfStopped(AC_MQ01_Opportunity_MusicianPerformance)
EndFunction

Function Fragment_Stage_0590_Item_00()
    Actor musician = ActorFromAlias(Alias_Actor_Musician)
    If musician != None
        musician.SetGhost(False)
        musician.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    Quests:AC_MQ01_Opportunity:QuestScript controller = QuestController()
    If controller != None
        controller.EndPerformanceDucking()
    EndIf
    SetHitmenCombatState(True)
EndFunction

Function Fragment_Stage_0670_Item_00()
    TryFinishHitmanFight()
EndFunction

Function Fragment_Stage_0680_Item_00()
    TryFinishHitmanFight()
EndFunction

Function Fragment_Stage_0690_Item_00()
    TryFinishHitmanFight()
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    Actor leader = ActorFromAlias(Alias_Actor_HitmanLeader)
    If leader != None
        leader.StopCombat()
        leader.SetGhost(False)
        leader.RemoveFromFaction(AC_MQ01_Opportunity_EnemyFaction)
        leader.AddToFaction(CaptiveFaction)
        leader.EvaluatePackage()
    EndIf
    StartSceneIfStopped(AC_MQ01_Opportunity_AfterFightCommentary)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
    StartSceneIfStopped(AC_MQ01_Opportunity_AttackAftermath)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
    Actor evelyn = ActorFromAlias(Alias_Actor_Evelyn)
    ObjectReference destination = Alias_Marker_EvelynStormAway.GetReference()
    If evelyn != None && destination != None
        evelyn.MoveTo(destination)
        evelyn.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
    RestoreActor(Alias_Actor_Vin)
EndFunction

Function Fragment_Stage_1010_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
    RestoreActor(Alias_Actor_Evelyn)
EndFunction

Function Fragment_Stage_1015_Item_00()
    RestoreActor(Alias_Actor_Evelyn)
EndFunction

Function Fragment_Stage_1020_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    Actor evelyn = ActorFromAlias(Alias_Actor_Evelyn)
    ObjectReference microphone = Alias_Furn_EvelynMicrophone.GetReference()
    If evelyn != None
        evelyn.Enable()
        If microphone != None
            evelyn.MoveTo(microphone)
        EndIf
        evelyn.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1030_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    RestoreActor(Alias_Actor_Vin)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(130)
    Actor player = ActorFromAlias(Alias_Player)
    If AC_MQ02_Stage_StartKeyword != None && player != None && AC_MQ02_Stage != None && !AC_MQ02_Stage.IsRunning() && !AC_MQ02_Stage.IsCompleted()
        AC_MQ02_Stage_StartKeyword.SendStoryEvent(None, player, player)
    EndIf
EndFunction
