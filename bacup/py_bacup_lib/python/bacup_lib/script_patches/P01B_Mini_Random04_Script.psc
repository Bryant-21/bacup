Function RefreshClueProgress()
    Actor playerRef = alias_Player.GetReference() as Actor
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None
        Return
    EndIf

    Int locationOneClues = 0
    If IsStageDone(10)
        locationOneClues += 1
    EndIf
    If IsStageDone(11)
        locationOneClues += 1
    EndIf
    If IsStageDone(12)
        locationOneClues += 1
    EndIf
    If IsStageDone(13)
        locationOneClues += 1
    EndIf

    Int locationTwoClues = 0
    If IsStageDone(21)
        locationTwoClues += 1
    EndIf
    If IsStageDone(22)
        locationTwoClues += 1
    EndIf

    Int locationThreeClues = 0
    If IsStageDone(400)
        locationThreeClues = 1
    EndIf

    playerRef.SetValue(P01B_Mini_Random04_CluesFoundLoc1, locationOneClues as Float)
    playerRef.SetValue(P01B_Mini_Random04_CluesFoundLoc2, locationTwoClues as Float)
    playerRef.SetValue(P01B_Mini_Random04_CluesFoundLoc3, locationThreeClues as Float)

    If locationThreeClues >= MaxCluesLoc3
        If locationOneClues >= MaxCluesLoc1 && locationTwoClues >= MaxCluesLoc2
            If !IsStageDone(550)
                SetStage(550)
            EndIf
        ElseIf !IsStageDone(500)
            SetStage(500)
        EndIf
    ElseIf locationTwoClues >= MaxCluesLoc2 && !IsStageDone(300)
        SetStage(300)
    ElseIf locationOneClues >= MaxCluesLoc1 && !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction
