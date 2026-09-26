Function Fragment_Phase_01_End()
    Actor vertibot = Alias_AirDropVertibot.GetActorReference()
    If vertibot != None && VertibirdLand != None
        vertibot.SetValue(VertibirdLand, 1.0)
        vertibot.EvaluatePackage()
    EndIf
    ; The landed Cargobot releases the supply drop.
    FF11_Raid_AirDropScript airDrop = GetOwningQuest() as FF11_Raid_AirDropScript
    If airDrop != None
        airDrop.DropCargo()
    EndIf
EndFunction
