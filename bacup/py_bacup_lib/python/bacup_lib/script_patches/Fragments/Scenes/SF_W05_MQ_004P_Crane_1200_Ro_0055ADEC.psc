Function Fragment_End()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    ; "Someone is." (1220) carries EndRunningScene, so the fight phases that set 1230 never run.
    If owningQuest.IsStageDone(1220)
        If !owningQuest.IsStageDone(1230) && !owningQuest.IsStageDone(1250)
            owningQuest.SetStage(1230)
        EndIf
        Return
    EndIf
    If RoperResolved(owningQuest)
        Return
    EndIf
    ; The Charisma answer hands off to 1205_ConvincedRoperNoTreasure, which has no stage setter
    ; or fragment of its own; the server script released the Radicals once it finished.
    Scene noTreasureScene = Game.GetFormFromFile(0x0055ADED, "SeventySix.esm") as Scene
    Scene showWeaponScene = Game.GetFormFromFile(0x0055ADEE, "SeventySix.esm") as Scene
    Utility.Wait(1.0)
    If noTreasureScene == None || !noTreasureScene.IsPlaying() || IsPlaying() || (showWeaponScene != None && showWeaponScene.IsPlaying())
        Return
    EndIf
    Int waitedSeconds = 0
    While noTreasureScene.IsPlaying() && waitedSeconds < 120
        Utility.Wait(1.0)
        waitedSeconds += 1
    EndWhile
    If !owningQuest.IsRunning() || RoperResolved(owningQuest)
        Return
    EndIf
    owningQuest.SetStage(1260)
    If !owningQuest.IsStageDone(1300)
        owningQuest.SetStage(1300)
    EndIf
EndFunction

Bool Function RoperResolved(Quest owningQuest)
    Return owningQuest.IsStageDone(1221) || owningQuest.IsStageDone(1230) || owningQuest.IsStageDone(1235) || owningQuest.IsStageDone(1240) || owningQuest.IsStageDone(1242) || owningQuest.IsStageDone(1243) || owningQuest.IsStageDone(1244) || owningQuest.IsStageDone(1245) || owningQuest.IsStageDone(1250) || owningQuest.IsStageDone(1260)
EndFunction
