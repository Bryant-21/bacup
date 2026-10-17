Function Fragment_Begin(ObjectReference akSpeakerRef)
    MOON_Clyde_BartenderScript bar = GetOwningQuest() as MOON_Clyde_BartenderScript
    If bar == None || bar.Drinks_High == None || bar.Drinks_High.Length == 0
        Return
    EndIf
    MOON_Clyde_BartenderScript:DrinkDatum drink = bar.Drinks_High[Utility.RandomInt(0, bar.Drinks_High.Length - 1)]
    If drink == None || drink.DrinkPotion == None
        Return
    EndIf
    Game.GetPlayer().AddItem(drink.DrinkPotion, 1)
    If drink.DrinkMessage
        drink.DrinkMessage.Show()
    EndIf
EndFunction
