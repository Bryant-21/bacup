Function Fragment_Stage_0010_Item_00()
EndFunction

Function Fragment_Stage_0020_Item_00()
    ObjectReference carRef = Alias_Car.GetReference()
    If carRef != None
        carRef.DamageObject(9999.0)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
