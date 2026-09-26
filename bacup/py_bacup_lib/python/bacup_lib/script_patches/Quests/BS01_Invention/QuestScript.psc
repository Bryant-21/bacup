Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 300
        StartTimer(ValdezEntranceTime, ValdezEntranceTimerID)
    ElseIf auiStageID == 510 || auiStageID == 520 || auiStageID == 530
        If BS01_Invention_Valdez_FoundDoc_Scene != None && !BS01_Invention_Valdez_FoundDoc_Scene.IsPlaying()
            BS01_Invention_Valdez_FoundDoc_Scene.Start()
        EndIf
        If (auiStageID == 510 || IsStageDone(510)) && (auiStageID == 520 || IsStageDone(520)) && (auiStageID == 530 || IsStageDone(530)) && !IsStageDone(AllDocumentsCollectedStage)
            SetStage(AllDocumentsCollectedStage)
        EndIf
    ElseIf auiStageID == 740
        StartTimer(0.1, 740)
    ElseIf auiStageID >= 910 && auiStageID <= 933
        Bool cpuCollected = (auiStageID >= 910 && auiStageID <= 913) || IsStageDone(910) || IsStageDone(911) || IsStageDone(912) || IsStageDone(913)
        Bool ionCollected = (auiStageID >= 920 && auiStageID <= 923) || IsStageDone(920) || IsStageDone(921) || IsStageDone(922) || IsStageDone(923)
        Bool gaugeCollected = (auiStageID >= 930 && auiStageID <= 933) || IsStageDone(930) || IsStageDone(931) || IsStageDone(932) || IsStageDone(933)
        If cpuCollected && ionCollected && gaugeCollected && !IsStageDone(InitialComponentsGatheredStage)
            SetStage(InitialComponentsGatheredStage)
        EndIf
        If auiStageID == 913 || auiStageID == 923 || auiStageID == 933
            If BS01_MQ02_Invention_Valdez_PoorExtraction_Scene != None && !BS01_MQ02_Invention_Valdez_PoorExtraction_Scene.IsPlaying()
                BS01_MQ02_Invention_Valdez_PoorExtraction_Scene.Start()
            EndIf
        EndIf
    ElseIf auiStageID >= 1010 && auiStageID <= 1013
        If (auiStageID == 1012 || auiStageID == 1013) && BS01_MQ02_Invention_Valdez_PoorExtraction_Scene != None && !BS01_MQ02_Invention_Valdez_PoorExtraction_Scene.IsPlaying()
            BS01_MQ02_Invention_Valdez_PoorExtraction_Scene.Start()
        EndIf
        If !IsStageDone(UltraciteBatteryExtractionStage)
            SetStage(UltraciteBatteryExtractionStage)
        EndIf
    ElseIf auiStageID == UltraciteBatteryExtractionStage
        ObjectReference humMarker = Alias_UltraciteBatteryHum_SoundMarker.GetReference()
        If humMarker != None
            humMarker.Enable()
        EndIf
        Int steamIndex = 0
        While steamIndex < Alias_UltraciteBatterySteamFX_MovableStatics.GetCount()
            ObjectReference steamRef = Alias_UltraciteBatterySteamFX_MovableStatics.GetAt(steamIndex)
            If steamRef != None
                steamRef.Enable()
            EndIf
            steamIndex += 1
        EndWhile
        StartTimer(UltraciteBatterySteamTimerInterval, UltraciteBatterySteamTimerID)
    ElseIf auiStageID == 1200
        CancelTimer(UltraciteBatterySteamTimerID)
        ObjectReference humMarker = Alias_UltraciteBatteryHum_SoundMarker.GetReference()
        If humMarker != None
            humMarker.Disable()
        EndIf
        Int steamIndex = 0
        While steamIndex < Alias_UltraciteBatterySteamFX_MovableStatics.GetCount()
            ObjectReference steamRef = Alias_UltraciteBatterySteamFX_MovableStatics.GetAt(steamIndex)
            If steamRef != None
                steamRef.Disable()
            EndIf
            steamIndex += 1
        EndWhile
        ObjectReference extractionPoint = Alias_UltraciteBatteryExtractionPoint.GetReference()
        If extractionPoint != None && UltraciteBatteryEjectSound != None
            UltraciteBatteryEjectSound.Play(extractionPoint)
        EndIf
        ObjectReference playerRef = Alias_Player.GetReference()
        If playerRef != None && CameraShakeSpell != None
            CameraShakeSpell.Cast(playerRef, playerRef)
        EndIf
        StartTimer(0.1, 1200)
    ElseIf auiStageID == 750
        RefCollectionAlias wiringEnemies = GetAlias(35) as RefCollectionAlias
        If wiringEnemies != None
            Int enemyIndex = wiringEnemies.GetCount() - 1
            While enemyIndex >= 0
                Actor enemyRef = wiringEnemies.GetAt(enemyIndex) as Actor
                If enemyRef != None
                    UnregisterForRemoteEvent(enemyRef, "OnDeath")
                    wiringEnemies.RemoveRef(enemyRef)
                    enemyRef.Disable()
                    enemyRef.Delete()
                EndIf
                enemyIndex -= 1
            EndWhile
        EndIf
    ElseIf auiStageID == EWSEndStage
        If Alias_UltraciteFight_Enemies_RefCollection != None
            Int enemyIndex = Alias_UltraciteFight_Enemies_RefCollection.GetCount() - 1
            While enemyIndex >= 0
                Actor enemyRef = Alias_UltraciteFight_Enemies_RefCollection.GetAt(enemyIndex) as Actor
                If enemyRef != None
                    UnregisterForRemoteEvent(enemyRef, "OnDeath")
                    Alias_UltraciteFight_Enemies_RefCollection.RemoveRef(enemyRef)
                    enemyRef.Disable()
                    enemyRef.Delete()
                EndIf
                enemyIndex -= 1
            EndWhile
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 740 || aiTimerID == 1200
        ReconcileLocalCombat(aiTimerID)
    ElseIf aiTimerID == ValdezEntranceTimerID
        If IsStageDone(300) && !IsStageDone(DungeonValdezEnableStage)
            SetStage(DungeonValdezEnableStage)
        EndIf
    ElseIf aiTimerID == UltraciteBatterySteamTimerID
        If IsStageDone(UltraciteBatteryExtractionStage) && !IsStageDone(1200)
            Int sparkIndex = 0
            While sparkIndex < Alias_UltraciteBatterySparkFX_Markers.GetCount()
                ObjectReference sparkMarker = Alias_UltraciteBatterySparkFX_Markers.GetAt(sparkIndex)
                If sparkMarker != None && UltraciteBatterySparkExplosion != None
                    sparkMarker.PlaceAtMe(UltraciteBatterySparkExplosion)
                EndIf
                sparkIndex += 1
            EndWhile
            StartTimer(UltraciteBatterySteamTimerInterval, UltraciteBatterySteamTimerID)
        EndIf
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If IsStageDone(740) && !IsStageDone(750)
        RefCollectionAlias wiringEnemies = GetAlias(35) as RefCollectionAlias
        Bool foundWiringEnemy = False
        Bool allWiringEnemiesDead = True
        If wiringEnemies != None
            Int wiringEnemyIndex = 0
            While wiringEnemyIndex < wiringEnemies.GetCount()
                Actor wiringEnemyRef = wiringEnemies.GetAt(wiringEnemyIndex) as Actor
                If wiringEnemyRef != None
                    foundWiringEnemy = True
                    If !wiringEnemyRef.IsDead()
                        allWiringEnemiesDead = False
                    EndIf
                EndIf
                wiringEnemyIndex += 1
            EndWhile
        EndIf
        If foundWiringEnemy && allWiringEnemiesDead
            SetStage(750)
        EndIf
    EndIf
    If IsStageDone(1200) && !IsStageDone(EWSEndStage)
        Bool foundRobotEnemy = False
        Bool allEnemiesDead = True
        Int enemyIndex = 0
        While enemyIndex < Alias_UltraciteFight_Enemies_RefCollection.GetCount()
            Actor enemyRef = Alias_UltraciteFight_Enemies_RefCollection.GetAt(enemyIndex) as Actor
            If enemyRef != None
                foundRobotEnemy = True
                If !enemyRef.IsDead()
                    allEnemiesDead = False
                EndIf
            EndIf
            enemyIndex += 1
        EndWhile
        If foundRobotEnemy && allEnemiesDead
            SetStage(EWSEndStage)
        EndIf
    EndIf
EndEvent

Function ReconcileLocalCombat(Int startStage)
    If !IsRunning()
        Return
    EndIf
    RefCollectionAlias enemies
    Int completionStage
    If startStage == 740
        enemies = GetAlias(35) as RefCollectionAlias
        completionStage = 750
    ElseIf startStage == 1200
        enemies = Alias_UltraciteFight_Enemies_RefCollection
        completionStage = EWSEndStage
    Else
        Return
    EndIf
    If IsStageDone(completionStage)
        Return
    EndIf
    If !IsStageDone(startStage)
        StartTimer(0.1, startStage)
        Return
    EndIf
    If enemies == None
        Return
    EndIf
    Bool foundEnemy = False
    Bool allDead = True
    Int index = 0
    While index < enemies.GetCount()
        Actor enemy = enemies.GetAt(index) as Actor
        If enemy != None
            foundEnemy = True
            If !enemy.IsDead()
                allDead = False
                RegisterForRemoteEvent(enemy, "OnDeath")
            EndIf
        EndIf
        index += 1
    EndWhile
    If foundEnemy && allDead
        SetStage(completionStage)
    EndIf
EndFunction
