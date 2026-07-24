Function Fragment_End(ObjectReference akSpeakerRef)
    If LL_Recipes_Cooking_Gourmet != None
        Game.GetPlayer().AddItem(LL_Recipes_Cooking_Gourmet, 1)
    EndIf
EndFunction
