Function SetBadGuyState(Float afState)
    If BadGuy == None || W05_RE_ObjectBB02_BadGuyAV == None
        Return
    EndIf

    Actor kBadGuy = BadGuy.GetActorReference()
    If kBadGuy == None
        Return
    EndIf

    kBadGuy.SetValue(W05_RE_ObjectBB02_BadGuyAV, afState)
    kBadGuy.EvaluatePackage(False)
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If !IsRunning()
        Return
    EndIf

    If auiStageID == 100
        SetBadGuyState(1.0)
    ElseIf auiStageID == 200
        SetBadGuyState(2.0)
    ElseIf auiStageID == 250
        SetBadGuyState(3.0)
    EndIf
EndEvent
