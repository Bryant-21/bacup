Function Fragment_Begin(ObjectReference akSpeakerRef)
    MOON_Clyde_BartenderScript bar = GetOwningQuest() as MOON_Clyde_BartenderScript
    If bar == None || bar.Drinks_Mid == None || bar.Drinks_Mid.Length == 0
        Return
    EndIf
    MOON_Clyde_BartenderScript:DrinkDatum drink = bar.Drinks_Mid[Utility.RandomInt(0, bar.Drinks_Mid.Length - 1)]
    If drink == None || drink.DrinkPotion == None
        Return
    EndIf
    Game.GetPlayer().AddItem(drink.DrinkPotion, 1)
    If drink.DrinkMessage
        drink.DrinkMessage.Show()
    EndIf
EndFunction
