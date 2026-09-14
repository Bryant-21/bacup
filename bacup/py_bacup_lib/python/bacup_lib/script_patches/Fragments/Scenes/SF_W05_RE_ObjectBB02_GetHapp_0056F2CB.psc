Function Fragment_Phase_02_End()
    If Alias_BadGuy == None || W05_RE_ObjectBB02_BadGuyAV == None
        Return
    EndIf

    Actor kBadGuy = Alias_BadGuy.GetActorReference()
    If kBadGuy != None
        kBadGuy.SetValue(W05_RE_ObjectBB02_BadGuyAV, 0.0)
    EndIf
EndFunction
