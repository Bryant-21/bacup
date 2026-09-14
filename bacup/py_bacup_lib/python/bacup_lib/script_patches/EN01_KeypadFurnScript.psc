Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        UseKeypad()
    EndIf
EndEvent

Function UseKeypad()
    Quest bunkerQuest = GetOwningQuest()
    Actor playerRef = Game.GetPlayer()
    If bunkerQuest == None || playerRef == None || bunkerQuest.IsStageDone(150)
        Return
    EndIf
    If !bunkerQuest.IsStageDone(iActivatedKeypadStage)
        bunkerQuest.SetStage(iActivatedKeypadStage)
    EndIf
    If playerRef.GetValue(EN01_PlayerActivatedSanctumKeypadOnce) < 1.0
        playerRef.SetValue(EN01_PlayerActivatedSanctumKeypadOnce, 1.0)
        EN01_Sam_SanctumKeypadIntroScene.Start()
    EndIf
    If bunkerQuest.IsStageDone(112)
        If !bunkerQuest.IsStageDone(122)
            bunkerQuest.SetStage(122)
        EndIf
        EN01_Sam_SanctumKeypadCorrect.Start()
        bunkerQuest.SetStage(150)
    ElseIf bunkerQuest.IsStageDone(115) && !bunkerQuest.IsStageDone(124)
        bunkerQuest.SetStage(124)
        EN01_Sam_SanctumKeypadIncorrect.Start()
    ElseIf bunkerQuest.IsStageDone(117) && !bunkerQuest.IsStageDone(126)
        bunkerQuest.SetStage(126)
        EN01_Sam_SanctumKeypadIncorrect.Start()
    ElseIf bunkerQuest.IsStageDone(119) && !bunkerQuest.IsStageDone(128)
        bunkerQuest.SetStage(128)
        EN01_Sam_SanctumKeypadIncorrect.Start()
    Else
        EN01_Sam_SanctumKeypadIncorrect.Start()
    EndIf
EndFunction
