Function Fragment_End(Actor akActor)
    If akActor != None && E01B_Herd_ScaredBrahminKeyword != None
        akActor.RemoveKeyword(E01B_Herd_ScaredBrahminKeyword)
        akActor.EvaluatePackage()
    EndIf
EndFunction
