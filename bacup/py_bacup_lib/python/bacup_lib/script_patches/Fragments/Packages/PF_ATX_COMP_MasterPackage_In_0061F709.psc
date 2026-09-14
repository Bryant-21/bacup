Function Fragment_Begin(Actor akActor)
    If akActor == None
        Return
    EndIf
    If InspectorDaphne_TransformFX != None
        akActor.PlaceAtMe(InspectorDaphne_TransformFX)
    EndIf
    If TransformTime > 0.0
        Utility.Wait(TransformTime)
    EndIf
    If newClothes != None
        akActor.EquipItem(newClothes, False, True)
    EndIf
EndFunction
