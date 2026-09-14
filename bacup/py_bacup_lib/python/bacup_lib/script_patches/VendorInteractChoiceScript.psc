Function TriggerVendorInteraction(Actor currentPlayer, ObjectReference vendor)
    If currentPlayer != Game.GetPlayer() || vendor == None || Utility.IsInMenuMode()
        Return
    EndIf

    Actor vendorActor = vendor as Actor
    Form vendorBase = vendor.GetBaseObject()
    If vendorActor == None || vendorBase == None
        Return
    EndIf

    Keyword genericVendorKeyword = Game.GetFormFromFile(0x004F5632, "SeventySix.esm") as Keyword
    Keyword goldVendorKeyword = Game.GetFormFromFile(0x005A11A1, "SeventySix.esm") as Keyword
    Keyword stampsVendorKeyword = Game.GetFormFromFile(0x0065D4C8, "SeventySix.esm") as Keyword
    If vendorBase.HasKeyword(genericVendorKeyword) || vendorBase.HasKeyword(goldVendorKeyword) || vendorBase.HasKeyword(stampsVendorKeyword)
        Return
    EndIf

    Keyword craterVendorKeyword = Game.GetFormFromFile(0x005614E6, "SeventySix.esm") as Keyword
    Keyword foundationVendorKeyword = Game.GetFormFromFile(0x0059276B, "SeventySix.esm") as Keyword
    Keyword waywardVendorKeyword = Game.GetFormFromFile(0x00593DCF, "SeventySix.esm") as Keyword
    If vendorBase.HasKeyword(craterVendorKeyword) || vendorBase.HasKeyword(foundationVendorKeyword) || vendorBase.HasKeyword(waywardVendorKeyword)
        Utility.Wait(0.2)
        vendorActor.ShowBarterMenu()
    EndIf
EndFunction
