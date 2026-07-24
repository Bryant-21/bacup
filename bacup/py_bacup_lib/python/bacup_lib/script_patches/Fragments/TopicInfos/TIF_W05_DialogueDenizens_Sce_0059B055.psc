Function Fragment_End(ObjectReference akSpeakerRef)
    If LL_Recipes_Cooking_Tasty != None
        Game.GetPlayer().AddItem(LL_Recipes_Cooking_Tasty, 1)
    EndIf
EndFunction
