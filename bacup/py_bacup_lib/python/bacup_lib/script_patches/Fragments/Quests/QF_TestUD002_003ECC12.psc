Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
    SetObjectiveDisplayed(150, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0225_Item_00()
    SetObjectiveCompleted(210, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveCompleted(210, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(150, True)
    SetObjectiveCompleted(300, True)
    SetObjectiveCompleted(310, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveDisplayed(310, True)
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveCompleted(310, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_ActivePlayer.GetReference() as Actor
    Location burrowsLocation = Alias_BurrowsLocation.GetLocation()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If UD002_BossWave_Keyword != None && playerRef != None
        UD002_BossWave_Keyword.SendStoryEventAndWait(burrowsLocation, playerRef)
    EndIf

    defaultquestencounterwavescript encounterWaves = UD002_Burrows as defaultquestencounterwavescript
    If encounterWaves != None
        encounterWaves.StartLocalEncounterWave(0)
        encounterWaves.StartLocalEncounterWave(1)
    EndIf

    ReferenceAlias controllerBossAlias = UD002_Burrows.GetAlias(3) as ReferenceAlias
    ObjectReference controllerBoss
    If controllerBossAlias != None
        controllerBoss = controllerBossAlias.GetReference()
    EndIf
    If controllerBoss != None
        Alias_Boss.ForceRefTo(controllerBoss)
    EndIf

    Int controllerAliasIndex = 8
    While controllerAliasIndex <= 12
        RefCollectionAlias controllerCollection = UD002_Burrows.GetAlias(controllerAliasIndex) as RefCollectionAlias
        If controllerCollection != None
            Int actorIndex = 0
            While actorIndex < controllerCollection.GetCount()
                ObjectReference actorRef = controllerCollection.GetAt(actorIndex)
                If actorRef != None && Alias_MinionCollection.Find(actorRef) < 0 && actorRef != controllerBoss
                    Alias_MinionCollection.AddRef(actorRef)
                EndIf
                actorIndex += 1
            EndWhile
        EndIf
        If controllerAliasIndex == 8
            controllerAliasIndex = 11
        Else
            controllerAliasIndex += 1
        EndIf
    EndWhile

    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    Int livingMinions = 0
    Int minionIndex = 0
    While minionIndex < Alias_MinionCollection.GetCount()
        Actor minionRef = Alias_MinionCollection.GetAt(minionIndex) as Actor
        If minionRef != None && !minionRef.IsDead()
            livingMinions += 1
        EndIf
        minionIndex += 1
    EndWhile
    SetObjectiveCompleted(500, True)
    If livingMinions > 0
        SetStage(600)
    Else
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600, True)
    Int livingMinions = 0
    Int minionIndex = 0
    While minionIndex < Alias_MinionCollection.GetCount()
        Actor minionRef = Alias_MinionCollection.GetAt(minionIndex) as Actor
        If minionRef != None && !minionRef.IsDead()
            livingMinions += 1
        EndIf
        minionIndex += 1
    EndWhile
    If livingMinions <= 0
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(150, True)
    SetObjectiveCompleted(200, True)
    SetObjectiveCompleted(210, True)
    SetObjectiveCompleted(300, True)
    SetObjectiveCompleted(310, True)
    SetObjectiveCompleted(400, True)
    SetObjectiveCompleted(500, True)
    SetObjectiveCompleted(600, True)
    CompleteQuest()
    If UD002_Burrows != None && UD002_Burrows.IsRunning()
        UD002_Burrows.Stop()
    EndIf
EndFunction
