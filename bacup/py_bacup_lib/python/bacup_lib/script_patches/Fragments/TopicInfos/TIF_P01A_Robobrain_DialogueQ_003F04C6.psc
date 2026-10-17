Function Fragment_End(ObjectReference akSpeakerRef)
    ; This greeting shares INFO group 3EC1A8 with four siblings that bind D01A_Brewing_MasterQuest;
    ; FO76 left this one without the property, so resolve the same quest to keep the daily offer.
    Quests:U01A_Brewing:MasterScript masterScript = Game.GetFormFromFile(0x0047F037, "SeventySix.esm") as Quests:U01A_Brewing:MasterScript
    If masterScript != None
        masterScript.ChooseAndStartDailyQuest()
    EndIf
EndFunction
